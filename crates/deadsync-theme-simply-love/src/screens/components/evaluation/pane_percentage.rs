use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::screens::evaluation::{EvalPane, ScoreInfo};
use deadlib_present::actors::{Actor, SizeSpec, TextContent};
use deadlib_present::color;
use deadlib_present::color::{JudgmentColorRole as Role, JudgmentPalette};
use deadsync_profile as profile_data;
use std::cell::RefCell;
use std::sync::Arc;

use super::utils::{eval_style_alpha, pane_origin_x, pane3_origin_x};

// Simply Love uses Wendy/_wendy white for evaluation percentages. Arrow Cloud's
// Mega path uses ThemeFont Bold with larger zooms so the narrower Mega glyphs
// fill the same score boxes.
const SCORE_ZOOM_WENDY: f32 = 0.585;
const SCORE_ZOOM_MEGA: f32 = 0.95;
const SMALL_SCORE_ZOOM_WENDY: f32 = 0.25;
const SMALL_SCORE_ZOOM_MEGA: f32 = 0.406;
const COMPANION_SCORE_ZOOM_WENDY: f32 = 0.32;
const COMPANION_SCORE_ZOOM_MEGA: f32 = 0.52;

#[derive(Clone, Copy, PartialEq)]
struct PercentageCacheKey {
    show_ex_score: bool,
    column_count: usize,
    pane: EvalPane,
    controller: profile_data::PlayerSide,
    transparent: bool,
    machine_font: MachineFont,
    palette: JudgmentPalette,
}

#[derive(Clone)]
pub(crate) struct PercentageText {
    score: TextContent,
    ex: TextContent,
    hard_ex: TextContent,
    cached: RefCell<Option<(PercentageCacheKey, Arc<[Actor]>)>>,
}

impl PercentageText {
    pub(crate) fn new(score: &ScoreInfo) -> Self {
        Self {
            score: percent_text(score.score_percent * 100.0),
            ex: percent_text(score.ex_score_percent.max(0.0)),
            hard_ex: percent_text(score.hard_ex_score_percent.max(0.0)),
            cached: RefCell::new(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn cached_actors(
        &self,
        show_ex_score: bool,
        column_count: usize,
        pane: EvalPane,
        controller: profile_data::PlayerSide,
        transparent: bool,
        machine_font: MachineFont,
        palette: JudgmentPalette,
    ) -> Arc<[Actor]> {
        let key = PercentageCacheKey {
            show_ex_score,
            column_count,
            pane,
            controller,
            transparent,
            machine_font,
            palette,
        };
        if let Some((_, actors)) = self
            .cached
            .borrow()
            .as_ref()
            .filter(|(cached, _)| *cached == key)
        {
            return Arc::clone(actors);
        }

        let actors = Arc::from(
            pane_percentage_actor(
                show_ex_score,
                column_count,
                self,
                pane,
                controller,
                transparent,
                machine_font,
                palette,
                4,
            )
            .into_iter()
            .collect::<Vec<_>>(),
        );
        *self.cached.borrow_mut() = Some((key, Arc::clone(&actors)));
        actors
    }
}

#[inline]
fn percent_text(value: f64) -> TextContent {
    super::retained_text(format_args!("{value:.2}"))
}

#[inline(always)]
const fn choose_score_zoom(machine_font: MachineFont, wendy: f32, mega: f32) -> f32 {
    match machine_font {
        MachineFont::Wendy => wendy,
        MachineFont::Mega => mega,
    }
}

#[allow(clippy::too_many_arguments)]
fn pane_percentage_actor(
    show_ex_score: bool,
    column_count: usize,
    text: &PercentageText,
    pane: EvalPane,
    controller: profile_data::PlayerSide,
    transparent: bool,
    machine_font: MachineFont,
    palette: JudgmentPalette,
    child_capacity: usize,
) -> Option<Actor> {
    if matches!(
        pane,
        EvalPane::Timing
            | EvalPane::TimingEx
            | EvalPane::TimingHardEx
            | EvalPane::MachineRecords
            | EvalPane::QrCode
            | EvalPane::GrooveStats
            | EvalPane::GrooveStatsEx
            | EvalPane::Srpg
            | EvalPane::Itl
            | EvalPane::ArrowCloud
            | EvalPane::TestInput
    ) {
        return None;
    }

    let pane_origin_x = if pane == EvalPane::Column {
        pane3_origin_x(controller, column_count)
    } else {
        pane_origin_x(controller)
    };
    let cy = deadlib_present::space::screen_center_y();

    let score_bg_color = color::rgba_hex("#101519");
    let score_bg_alpha = eval_style_alpha(transparent, 1.0, 0.5);
    let score_zoom = choose_score_zoom(machine_font, SCORE_ZOOM_WENDY, SCORE_ZOOM_MEGA);
    let small_score_zoom =
        choose_score_zoom(machine_font, SMALL_SCORE_ZOOM_WENDY, SMALL_SCORE_ZOOM_MEGA);
    let companion_score_zoom = choose_score_zoom(
        machine_font,
        COMPANION_SCORE_ZOOM_WENDY,
        COMPANION_SCORE_ZOOM_MEGA,
    );

    let (bg_align_x, bg_x, percent_x) = if controller == profile_data::PlayerSide::P1 {
        (0.0, -150.0, 1.5)
    } else {
        (1.0, 150.0, 141.0)
    };

    let mut frame_x = pane_origin_x;
    let mut frame_y = cy - 26.0;
    let mut children = Vec::with_capacity(child_capacity);

    match pane {
        EvalPane::Timing => {}
        EvalPane::TimingEx => {}
        EvalPane::TimingHardEx => {}
        EvalPane::TimingArrows => {}
        EvalPane::MachineRecords => {}
        EvalPane::QrCode => {}
        EvalPane::GrooveStats => {}
        EvalPane::GrooveStatsEx => {}
        EvalPane::Srpg => {}
        EvalPane::Itl => {}
        EvalPane::ArrowCloud => {}
        EvalPane::TestInput => {}
        EvalPane::Column => {
            // Pane3 percentage container: small and not mirrored.
            frame_x = pane_origin_x - 115.0;
            frame_y = cy - 40.0;
            children.push(act!(quad:
                align(0.5, 0.5):
                xy(0.0, -2.0):
                setsize(70.0, 28.0):
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], score_bg_alpha)
            ));
            children.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Headline)):
                settext(text.score.clone()):
                align(1.0, 0.5):
                xy(30.0, -2.0):
                zoom(small_score_zoom):
                horizalign(right)
            ));
        }
        EvalPane::FaPlus => {
            let ex_color = palette.color(Role::FantasticBlue);
            let white = [1.0, 1.0, 1.0, 1.0];
            let (main_text, main_color, bottom_label, bottom_text, bottom_color) = if show_ex_score
            {
                (text.ex.clone(), ex_color, "ITG", text.score.clone(), white)
            } else {
                (text.score.clone(), white, "EX", text.ex.clone(), ex_color)
            };
            children.push(act!(quad:
                align(bg_align_x, 0.5):
                xy(bg_x, 14.0):
                setsize(158.5, 88.0):
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], score_bg_alpha)
            ));
            children.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Headline)):
                settext(main_text):
                align(1.0, 0.5):
                xy(percent_x, 0.0):
                zoom(score_zoom):
                horizalign(right):
                diffuse(main_color[0], main_color[1], main_color[2], main_color[3])
            ));

            // Simply Love Pane2 draws this companion score through
            // JudgmentLabels.lua and JudgmentNumbers.lua. These are the final
            // pane-local anchors after converting the label frame and the
            // number frame's 0.8 zoom into this shared percentage frame.
            let (bottom_label_x, bottom_value_x) = if controller == profile_data::PlayerSide::P1 {
                (-110.0, -1.2)
            } else {
                (32.0, 138.8)
            };
            children.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Bold)):
                settext(bottom_label):
                align(1.0, 0.5):
                xy(bottom_label_x, 40.0):
                zoom(0.5):
                horizalign(right):
                diffuse(bottom_color[0], bottom_color[1], bottom_color[2], bottom_color[3])
            ));
            children.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Headline)):
                settext(bottom_text):
                align(1.0, 0.5):
                xy(bottom_value_x, 39.6):
                zoom(companion_score_zoom):
                horizalign(right):
                diffuse(bottom_color[0], bottom_color[1], bottom_color[2], bottom_color[3])
            ));
        }
        EvalPane::HardEx => {
            children.push(act!(quad:
                align(bg_align_x, 0.5):
                xy(bg_x, 14.0):
                setsize(158.5, 88.0):
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], score_bg_alpha)
            ));

            let ex_color = palette.color(Role::FantasticBlue);
            let hex_color = color::HARD_EX_SCORE_RGBA;
            children.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Headline)):
                settext(text.ex.clone()):
                align(1.0, 0.5):
                xy(percent_x, 0.0):
                zoom(score_zoom):
                horizalign(right):
                diffuse(ex_color[0], ex_color[1], ex_color[2], ex_color[3])
            ));

            let bottom_value_x = if controller == profile_data::PlayerSide::P1 {
                0.0
            } else {
                percent_x
            };
            let bottom_label_x = bottom_value_x - 92.0;
            children.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Bold)):
                settext("H.EX"):
                align(1.0, 0.5):
                xy(bottom_label_x, 40.0):
                zoom(0.5):
                horizalign(right):
                diffuse(hex_color[0], hex_color[1], hex_color[2], hex_color[3])
            ));
            children.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Headline)):
                settext(text.hard_ex.clone()):
                align(1.0, 0.5):
                xy(bottom_value_x, 40.0):
                zoom(companion_score_zoom):
                horizalign(right):
                diffuse(hex_color[0], hex_color[1], hex_color[2], hex_color[3])
            ));
        }
        EvalPane::Standard => {
            children.push(act!(quad:
                align(bg_align_x, 0.5):
                xy(bg_x, 0.0):
                setsize(158.5, 60.0):
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], score_bg_alpha)
            ));
            children.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Headline)):
                settext(text.score.clone()):
                align(1.0, 0.5):
                xy(percent_x, 0.0):
                zoom(score_zoom):
                horizalign(right)
            ));
        }
    }

    Some(Actor::Frame {
        align: [0.5, 0.5],
        offset: [frame_x, frame_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z: 102,
        children,
    })
}

/// Appends the percentage pane without allocating a temporary outer actor list.
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_pane_percentage_display_with_palette(
    out: &mut Vec<Actor>,
    score_info: &ScoreInfo,
    text: &PercentageText,
    pane: EvalPane,
    controller: profile_data::PlayerSide,
    transparent: bool,
    machine_font: MachineFont,
    palette: JudgmentPalette,
) {
    let children = text.cached_actors(
        score_info.show_ex_score,
        score_info.column_judgments.len(),
        pane,
        controller,
        transparent,
        machine_font,
        palette,
    );
    if children.is_empty() {
        return;
    }
    out.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z: 0,
        tint: [1.0; 4],
        blend: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentage_text_formats_normal_values_inline() {
        let text = percent_text(98.765);
        assert_eq!(text.as_str(), "98.77");
        assert!(matches!(text, TextContent::Inline(_)));

        let text = percent_text(f64::NAN);
        assert_eq!(text.as_str(), "NaN");
        assert!(matches!(text, TextContent::Inline(_)));
    }

    #[test]
    fn percentage_text_shares_oversized_fallback() {
        let text = percent_text(f64::MAX);
        let clone = text.clone();
        assert_eq!(text.as_str(), format!("{:.2}", f64::MAX));
        let (TextContent::Shared(text), TextContent::Shared(clone)) = (text, clone) else {
            panic!("oversized percentages should use shared text");
        };
        assert!(std::sync::Arc::ptr_eq(&text, &clone));
    }
}
