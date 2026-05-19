use crate::act;
use crate::assets::{FontRole, current_machine_font_key};
use crate::config::MachineFont;
use crate::engine::present::actors::{Actor, SizeSpec};
use crate::engine::present::color;
use crate::game::profile;
use crate::screens::evaluation::{EvalPane, ScoreInfo};

use super::utils::pane_origin_x;

// Simply Love uses Wendy/_wendy white for evaluation percentages. Arrow Cloud's
// Mega path uses ThemeFont Bold with larger zooms so the narrower Mega glyphs
// fill the same score boxes.
const SCORE_ZOOM_WENDY: f32 = 0.585;
const SCORE_ZOOM_MEGA: f32 = 0.95;
const SMALL_SCORE_ZOOM_WENDY: f32 = 0.25;
const SMALL_SCORE_ZOOM_MEGA: f32 = 0.406;
const COMPANION_SCORE_ZOOM_WENDY: f32 = 0.32;
const COMPANION_SCORE_ZOOM_MEGA: f32 = 0.52;

#[inline(always)]
const fn choose_score_zoom(machine_font: MachineFont, wendy: f32, mega: f32) -> f32 {
    match machine_font {
        MachineFont::Wendy => wendy,
        MachineFont::Mega => mega,
    }
}

pub(crate) fn build_pane_percentage_display(
    score_info: &ScoreInfo,
    pane: EvalPane,
    controller: profile::PlayerSide,
) -> Vec<Actor> {
    if matches!(
        pane,
        EvalPane::Timing
            | EvalPane::TimingEx
            | EvalPane::TimingHardEx
            | EvalPane::MachineRecords
            | EvalPane::QrCode
            | EvalPane::GrooveStats
            | EvalPane::GrooveStatsEx
            | EvalPane::Itl
            | EvalPane::ArrowCloud
            | EvalPane::TestInput
    ) {
        return vec![];
    }

    let pane_origin_x = pane_origin_x(controller);
    let cy = crate::engine::space::screen_center_y();

    let percent_text = format!("{:.2}", score_info.score_percent * 100.0);
    let ex_percent_text = format!("{:.2}", score_info.ex_score_percent.max(0.0));
    let hard_ex_percent_text = format!("{:.2}", score_info.hard_ex_score_percent.max(0.0));
    let score_bg_color = color::rgba_hex("#101519");
    let machine_font = crate::config::get().machine_font;
    let score_zoom = choose_score_zoom(machine_font, SCORE_ZOOM_WENDY, SCORE_ZOOM_MEGA);
    let small_score_zoom =
        choose_score_zoom(machine_font, SMALL_SCORE_ZOOM_WENDY, SMALL_SCORE_ZOOM_MEGA);
    let companion_score_zoom = choose_score_zoom(
        machine_font,
        COMPANION_SCORE_ZOOM_WENDY,
        COMPANION_SCORE_ZOOM_MEGA,
    );

    let (bg_align_x, bg_x, percent_x) = if controller == profile::PlayerSide::P1 {
        (0.0, -150.0, 1.5)
    } else {
        (1.0, 150.0, 141.0)
    };

    let mut frame_x = pane_origin_x;
    let mut frame_y = cy - 26.0;
    let mut children: Vec<Actor> = Vec::new();

    match pane {
        EvalPane::Timing => {}
        EvalPane::TimingEx => {}
        EvalPane::TimingHardEx => {}
        EvalPane::MachineRecords => {}
        EvalPane::QrCode => {}
        EvalPane::GrooveStats => {}
        EvalPane::GrooveStatsEx => {}
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
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], 1.0)
            ));
            children.push(act!(text:
                font(current_machine_font_key(FontRole::Headline)):
                settext(percent_text):
                align(1.0, 0.5):
                xy(30.0, -2.0):
                zoom(small_score_zoom):
                horizalign(right)
            ));
        }
        EvalPane::FaPlus => {
            let ex_color = color::JUDGMENT_RGBA[0];
            let white = [1.0, 1.0, 1.0, 1.0];
            let (main_text, main_color, bottom_label, bottom_text, bottom_color) =
                if score_info.show_ex_score {
                    (
                        ex_percent_text.clone(),
                        ex_color,
                        "ITG",
                        percent_text.clone(),
                        white,
                    )
                } else {
                    (
                        percent_text.clone(),
                        white,
                        "EX",
                        ex_percent_text.clone(),
                        ex_color,
                    )
                };
            children.push(act!(quad:
                align(bg_align_x, 0.5):
                xy(bg_x, 14.0):
                setsize(158.5, 88.0):
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], 1.0)
            ));
            children.push(act!(text:
                font(current_machine_font_key(FontRole::Headline)):
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
            let (bottom_label_x, bottom_value_x) = if controller == profile::PlayerSide::P1 {
                (-110.0, -1.2)
            } else {
                (32.0, 138.8)
            };
            children.push(act!(text:
                font(current_machine_font_key(FontRole::Bold)):
                settext(bottom_label):
                align(1.0, 0.5):
                xy(bottom_label_x, 40.0):
                zoom(0.5):
                horizalign(right):
                diffuse(bottom_color[0], bottom_color[1], bottom_color[2], bottom_color[3])
            ));
            children.push(act!(text:
                font(current_machine_font_key(FontRole::Headline)):
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
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], 1.0)
            ));

            let ex_color = color::JUDGMENT_RGBA[0];
            let hex_color = color::HARD_EX_SCORE_RGBA;
            children.push(act!(text:
                font(current_machine_font_key(FontRole::Headline)):
                settext(ex_percent_text):
                align(1.0, 0.5):
                xy(percent_x, 0.0):
                zoom(score_zoom):
                horizalign(right):
                diffuse(ex_color[0], ex_color[1], ex_color[2], ex_color[3])
            ));

            let bottom_value_x = if controller == profile::PlayerSide::P1 {
                0.0
            } else {
                percent_x
            };
            let bottom_label_x = bottom_value_x - 92.0;
            children.push(act!(text:
                font(current_machine_font_key(FontRole::Bold)):
                settext("H.EX"):
                align(1.0, 0.5):
                xy(bottom_label_x, 40.0):
                zoom(0.5):
                horizalign(right):
                diffuse(hex_color[0], hex_color[1], hex_color[2], hex_color[3])
            ));
            children.push(act!(text:
                font(current_machine_font_key(FontRole::Headline)):
                settext(hard_ex_percent_text):
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
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], 1.0)
            ));
            children.push(act!(text:
                font(current_machine_font_key(FontRole::Headline)):
                settext(percent_text):
                align(1.0, 0.5):
                xy(percent_x, 0.0):
                zoom(score_zoom):
                horizalign(right)
            ));
        }
    }

    vec![Actor::Frame {
        align: [0.5, 0.5],
        offset: [frame_x, frame_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z: 102,
        children,
    }]
}
