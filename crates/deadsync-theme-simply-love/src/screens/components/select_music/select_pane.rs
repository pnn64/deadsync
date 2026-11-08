use crate::act;
use crate::fonts::machine_font_key;
use crate::i18n::tr;
use crate::ui_layout::UILayoutType;
use deadlib_present::actors::Actor;
use deadlib_present::space::{screen_height, screen_width};
use deadsync_config::theme::MachineFont;
use deadsync_theme::FontRole;
use std::sync::Arc;

const PANE_WIDTH_INSET: f32 = 10.0;
const PANE_BOTTOM_INSET: f32 = 32.0;
const PANE_HEIGHT: f32 = 60.0;
const STATS_LABEL_GAP: f32 = 3.0;
#[derive(Clone, Copy)]
struct PaneLayoutConstants {
    text_zoom: f32,
    scale_base: f32,
    scale_per_width: f32,
    meter_zoom: f32,
    stats_columns: [[f32; 2]; 2],
    score_columns: [[f32; 2]; 2],
    score_alignments: [f32; 2],
    score_max_widths: [Option<f32>; 2],
    cols: [f32; 4],
    rows: [f32; 3],
    value_max_width: Option<f32>,
    label_max_width: Option<f32>,
    meter_max_width: Option<f32>,
}

const SQUAREISH_LAYOUT: PaneLayoutConstants = PaneLayoutConstants {
    text_zoom: 0.8,
    scale_base: 0.0,
    scale_per_width: 1.0 / 310.0,
    meter_zoom: 0.48,
    stats_columns: [[0.0, 31.0], [1.0 / 3.0, 27.0]],
    score_columns: [[2.0 / 3.0, 2.0], [2.0 / 3.0, 28.0]],
    score_alignments: [0.0, 0.0],
    score_max_widths: [Some(24.0), Some(58.0)],
    cols: [-104.0, -36.0, 54.0, 150.0],
    rows: [13.0, 31.0, 49.0],
    value_max_width: Some(30.0),
    label_max_width: Some(44.0),
    meter_max_width: Some(66.0),
};

const WIDE_LAYOUT: PaneLayoutConstants = PaneLayoutConstants {
    text_zoom: 0.9,
    scale_base: 1.0,
    scale_per_width: 0.0,
    meter_zoom: 1.0,
    stats_columns: [[0.5, -133.0], [0.5, -38.0]],
    score_columns: [[0.5, 31.0], [0.5, 98.5]],
    score_alignments: [0.5, 1.0],
    score_max_widths: [Some(30.0), None],
    cols: [-133.0, -38.0, 76.0, 190.0],
    rows: [13.0, 31.0, 49.0],
    value_max_width: None,
    label_max_width: None,
    meter_max_width: None,
};

const ULTRA_WIDE_LAYOUT: PaneLayoutConstants = WIDE_LAYOUT;

fn layout_constants(layout_type: UILayoutType) -> &'static PaneLayoutConstants {
    match layout_type {
        UILayoutType::Squareish => &SQUAREISH_LAYOUT,
        UILayoutType::Wide => &WIDE_LAYOUT,
        UILayoutType::UltraWide => &ULTRA_WIDE_LAYOUT,
    }
}

#[derive(Clone, Copy)]
pub struct PaneLayout {
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    pub text_zoom: f32,
    pub stats_cols: [f32; 2],
    score_columns: [f32; 2],
    score_alignments: [f32; 2],
    score_max_widths: [Option<f32>; 2],
    pub cols: [f32; 4],
    pub rows: [f32; 3],
    value_max_width: Option<f32>,
    label_max_width: Option<f32>,
    meter_max_width: Option<f32>,
    scale: f32,
    meter_zoom: f32,
}

impl PaneLayout {
    pub fn from_center(center: [f32; 2], size: [f32; 2]) -> Self {
        let [width, height] = size;
        let constants = layout_constants(UILayoutType::current());
        let scale = scale_for_width(width, constants);
        Self {
            center_x: center[0],
            center_y: center[1],
            width,
            height,
            text_zoom: constants.text_zoom * scale,
            stats_cols: stats_value_columns_for(width, constants),
            score_columns: constants
                .score_columns
                .map(|[width_scale, offset]| width_scale.mul_add(width, offset * scale)),
            score_alignments: constants.score_alignments,
            score_max_widths: constants
                .score_max_widths
                .map(|max_width| max_width.map(|value| value * scale)),
            cols: constants.cols,
            rows: constants.rows.map(|value| value * scale),
            value_max_width: constants.value_max_width.map(|value| value * scale),
            label_max_width: constants.label_max_width.map(|value| value * scale),
            meter_max_width: constants.meter_max_width.map(|value| value * scale),
            scale,
            meter_zoom: constants.meter_zoom * scale,
        }
    }

    pub fn from_corners(top_left: [f32; 2], bottom_right: [f32; 2]) -> Self {
        let size = [bottom_right[0] - top_left[0], bottom_right[1] - top_left[1]];
        Self::from_center(
            [top_left[0] + size[0] * 0.5, top_left[1] + size[1] * 0.5],
            size,
        )
    }

    pub fn with_center_x(mut self, center_x: f32) -> Self {
        self.center_x = center_x;
        self
    }

    pub fn left(self) -> f32 {
        self.center_x - self.width * 0.5
    }

    pub fn right(self) -> f32 {
        self.center_x + self.width * 0.5
    }

    pub fn top(self) -> f32 {
        self.center_y - self.height * 0.5
    }

    pub fn bottom(self) -> f32 {
        self.center_y + self.height * 0.5
    }
}

pub struct StatsValues {
    pub steps: Arc<str>,
    pub mines: Arc<str>,
    pub jumps: Arc<str>,
    pub hands: Arc<str>,
    pub holds: Arc<str>,
    pub rolls: Arc<str>,
}

pub struct StatsPaneParams {
    pub machine_font: MachineFont,
    pub pane_cx: f32,
    pub accent_color: [f32; 4],
    pub values: StatsValues,
    pub meter: Option<Arc<str>>,
}

fn scale_for_width(pane_width: f32, constants: &PaneLayoutConstants) -> f32 {
    (constants.scale_base + constants.scale_per_width * pane_width).clamp(0.0, 1.0)
}

fn stats_value_columns_for(pane_width: f32, constants: &PaneLayoutConstants) -> [f32; 2] {
    let scale = scale_for_width(pane_width, constants);
    constants
        .stats_columns
        .map(|[width_scale, offset]| width_scale.mul_add(pane_width, offset * scale))
}

fn constrain_text_width(actor: &mut Actor, max_width: Option<f32>) {
    let Some(max_width) = max_width else {
        return;
    };
    if let Actor::Text {
        max_width: actor_max_width,
        max_w_pre_zoom,
        ..
    } = actor
    {
        *actor_max_width = Some(max_width);
        *max_w_pre_zoom = true;
    }
}

#[inline(always)]
#[must_use]
pub fn layout(pane_cx: f32) -> PaneLayout {
    let pane_width = screen_width() * 0.5 - PANE_WIDTH_INSET;
    let pane_center_y = screen_height() - PANE_BOTTOM_INSET - PANE_HEIGHT * 0.5;
    PaneLayout::from_center([pane_cx, pane_center_y], [pane_width, PANE_HEIGHT])
}

pub fn push_base(out: &mut Vec<Actor>, p: StatsPaneParams) {
    let StatsPaneParams {
        pane_cx,
        machine_font,
        accent_color,
        values,
        meter,
    } = p;
    let l = layout(pane_cx);
    out.push(act!(quad:
        align(0.5, 0.5):
        xy(l.center_x, l.center_y):
        setsize(l.width, l.height):
        z(120):
        diffuse(accent_color[0], accent_color[1], accent_color[2], 1.0)
    ));

    let stats = [
        (tr("Gameplay", "StatsSteps"), values.steps),
        (tr("Gameplay", "StatsMines"), values.mines),
        (tr("Gameplay", "StatsJumps"), values.jumps),
        (tr("Gameplay", "StatsHands"), values.hands),
        (tr("Gameplay", "StatsHolds"), values.holds),
        (tr("Gameplay", "StatsRolls"), values.rolls),
    ];
    for (i, (label, value)) in stats.into_iter().enumerate() {
        let (c, r) = (i % 2, i / 2);
        let value_x = l.left() + l.stats_cols[c];
        let label_x = value_x + STATS_LABEL_GAP * l.scale;
        let mut value_actor = act!(text:
            font("miso"):
            settext(value):
            align(1.0, 0.5):
            horizalign(right):
            xy(value_x, l.top() + l.rows[r]):
            zoom(l.text_zoom):
            z(121):
            diffuse(0.0, 0.0, 0.0, 1.0)
        );
        let mut label_actor = act!(text:
            font("miso"):
            settext(label):
            align(0.0, 0.5):
            xy(label_x, l.top() + l.rows[r]):
            zoom(l.text_zoom):
            z(121):
            diffuse(0.0, 0.0, 0.0, 1.0)
        );
        constrain_text_width(&mut value_actor, l.value_max_width);
        constrain_text_width(&mut label_actor, l.label_max_width);
        out.push(value_actor);
        out.push(label_actor);
    }

    if let Some(meter) = meter {
        let mut meter_actor = act!(text:
            font(machine_font_key(machine_font, FontRole::Header)):
            settext(meter):
            align(1.0, 0.5):
            horizalign(right):
            xy(l.right() - 6.0 * l.scale, l.top() + l.rows[1]):
            zoom(l.meter_zoom):
            z(121):
            diffuse(0.0, 0.0, 0.0, 1.0)
        );
        if let Some(max_width) = l.meter_max_width
            && let Actor::Text {
                max_width: actor_max_width,
                ..
            } = &mut meter_actor
        {
            *actor_max_width = Some(max_width);
        }
        out.push(meter_actor);
    }
}

pub fn push_score_line(
    out: &mut Vec<Actor>,
    pane: PaneLayout,
    row: usize,
    name: Arc<str>,
    score: Arc<str>,
) {
    let y = pane.top() + pane.rows[row];
    let mut name_actor = act!(text:
        font("miso"):
        settext(name):
        align(pane.score_alignments[0], 0.5):
        xy(pane.left() + pane.score_columns[0], y):
        zoom(pane.text_zoom):
        z(121):
        diffuse(0.0, 0.0, 0.0, 1.0)
    );
    let mut score_actor = act!(text:
        font("miso"):
        settext(score):
        align(pane.score_alignments[1], 0.5):
        xy(pane.left() + pane.score_columns[1], y):
        zoom(pane.text_zoom):
        z(121):
        diffuse(0.0, 0.0, 0.0, 1.0)
    );
    constrain_text_width(&mut name_actor, pane.score_max_widths[0]);
    constrain_text_width(&mut score_actor, pane.score_max_widths[1]);
    out.push(name_actor);
    out.push(score_actor);
}

#[cfg(test)]
mod tests {
    use super::{PaneLayout, layout_constants, stats_value_columns_for};
    use crate::ui_layout::UILayoutType;

    #[test]
    fn pane_layout_constructors_calculate_edges_consistently() {
        let from_center = PaneLayout::from_center([145.0, 350.0], [290.0, 60.0]);
        let from_corners = PaneLayout::from_corners([0.0, 320.0], [290.0, 380.0]);

        assert_eq!(from_center.left(), 0.0);
        assert_eq!(from_center.right(), 290.0);
        assert_eq!(from_center.top(), 320.0);
        assert_eq!(from_center.bottom(), 380.0);
        assert_eq!(from_center.left(), from_corners.left());
        assert_eq!(from_center.right(), from_corners.right());
        assert_eq!(from_center.top(), from_corners.top());
        assert_eq!(from_center.bottom(), from_corners.bottom());
    }

    #[test]
    fn narrow_stats_columns_fit_four_digit_values_and_labels() {
        for pane_width in [230.0, 310.0] {
            let scale = (pane_width / 310.0_f32).clamp(0.0, 1.0);
            let text_zoom = 0.8 * scale;
            let value_max_width = 30.0 * scale;
            let label_max_width = 44.0 * scale;
            let [left, right] =
                stats_value_columns_for(pane_width, layout_constants(UILayoutType::Squareish));
            let left_value_right = left;
            let right_value_right = right;
            let left_value_left = left_value_right - value_max_width * text_zoom;
            let left_label_right = left_value_right + 3.0 * scale + label_max_width * text_zoom;
            let right_value_left = right_value_right - value_max_width * text_zoom;
            let right_label_right = right_value_right + 3.0 * scale + label_max_width * text_zoom;
            let meter_left = pane_width - 6.0 * scale - 66.0 * scale * 0.48 * scale;

            assert!(left_value_left >= 0.0);
            assert!(left_label_right < right_value_left);
            assert!(right_label_right < meter_left);
        }
    }

    #[test]
    fn meter_is_anchored_inside_the_stats_pane_right_edge() {
        let pane = PaneLayout::from_center([145.0, 350.0], [290.0, 60.0]);

        assert_eq!(pane.right() - 8.0, 282.0);
    }

    #[test]
    fn stats_columns_are_selected_by_layout_family() {
        assert_eq!(
            stats_value_columns_for(310.0, layout_constants(UILayoutType::Squareish)),
            [31.0, 310.0 / 3.0 + 27.0]
        );
        assert_eq!(
            stats_value_columns_for(630.0, layout_constants(UILayoutType::Wide)),
            [182.0, 277.0]
        );
        assert_eq!(
            stats_value_columns_for(850.0, layout_constants(UILayoutType::UltraWide)),
            [292.0, 387.0]
        );
    }
}
