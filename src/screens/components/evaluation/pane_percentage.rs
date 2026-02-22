use crate::act;
use crate::game::profile;
use crate::screens::evaluation::{EvalPane, ScoreInfo};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;

use super::utils::pane_origin_x;

pub fn build_pane_percentage_display(
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
    ) {
        return vec![];
    }

    let pane_origin_x = pane_origin_x(controller);
    let cy = crate::core::space::screen_center_y();

    let percent_text = format!("{:.2}", score_info.score_percent * 100.0);
    let ex_percent_text = format!("{:.2}", score_info.ex_score_percent.max(0.0));
    let hard_ex_percent_text = format!("{:.2}", score_info.hard_ex_score_percent.max(0.0));
    let score_bg_color = color::rgba_hex("#101519");

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
                font("wendy_white"):
                settext(percent_text):
                align(1.0, 0.5):
                xy(30.0, -2.0):
                zoom(0.25):
                horizalign(right)
            ));
        }
        EvalPane::FaPlus => {
            children.push(act!(quad:
                align(bg_align_x, 0.5):
                xy(bg_x, 14.0):
                setsize(158.5, 88.0):
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], 1.0)
            ));
            children.push(act!(text:
                font("wendy_white"):
                settext(percent_text):
                align(1.0, 0.5):
                xy(percent_x, 0.0):
                zoom(0.585):
                horizalign(right)
            ));

            let ex_color = color::JUDGMENT_RGBA[0];
            let bottom_value_x = if controller == profile::PlayerSide::P1 {
                0.0
            } else {
                percent_x
            };
            let bottom_label_x = bottom_value_x - 108.0;
            children.push(act!(text:
                font("wendy_white"):
                settext("EX"):
                align(1.0, 0.5):
                xy(bottom_label_x, 40.0):
                zoom(0.31):
                horizalign(right):
                diffuse(ex_color[0], ex_color[1], ex_color[2], ex_color[3])
            ));
            children.push(act!(text:
                font("wendy_white"):
                settext(ex_percent_text):
                align(1.0, 0.5):
                xy(bottom_value_x, 40.0):
                zoom(0.31):
                horizalign(right):
                diffuse(ex_color[0], ex_color[1], ex_color[2], ex_color[3])
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
                font("wendy_white"):
                settext(ex_percent_text):
                align(1.0, 0.5):
                xy(percent_x, 0.0):
                zoom(0.585):
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
                font("wendy_white"):
                settext("H.EX"):
                align(1.0, 0.5):
                xy(bottom_label_x, 40.0):
                zoom(0.31):
                horizalign(right):
                diffuse(hex_color[0], hex_color[1], hex_color[2], hex_color[3])
            ));
            children.push(act!(text:
                font("wendy_white"):
                settext(hard_ex_percent_text):
                align(1.0, 0.5):
                xy(bottom_value_x, 40.0):
                zoom(0.31):
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
                font("wendy_white"):
                settext(percent_text):
                align(1.0, 0.5):
                xy(percent_x, 0.0):
                zoom(0.585):
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
