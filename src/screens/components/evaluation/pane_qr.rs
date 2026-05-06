use crate::act;
use crate::assets::{FontRole, current_machine_font_key};
use crate::engine::present::actors::{Actor, SizeSpec};
use crate::engine::present::color;
use crate::game::profile;
use crate::screens::components::shared::qr_code;
use crate::screens::evaluation::ScoreInfo;

use super::utils::pane_origin_x;

const MACHINE_RECORD_DEFAULT_ROW_HEIGHT: f32 = 22.0;
const GS_QR_INVALID_URL: &str = "https://www.youtube.com/watch?v=FMABVVk4Ge4";
const GS_QR_TITLE: &str = "GrooveStats QR";
const GS_QR_HELP_TEXT_VALID: &str =
    "Scan with your phone\nto upload this score\nto your GrooveStats\naccount.";
const GS_QR_FALLBACK_TEXT: &str = "QR Unavailable";

pub fn build_gs_qr_pane(score_info: &ScoreInfo, controller: profile::PlayerSide) -> Vec<Actor> {
    let gs_valid = score_info.groovestats.valid;
    let help_text = if gs_valid {
        GS_QR_HELP_TEXT_VALID.to_string()
    } else if score_info.groovestats.reason_lines.is_empty() {
        "This score is invalid for GrooveStats.".to_string()
    } else {
        score_info.groovestats.reason_lines.join("\n")
    };
    let pane_origin_x = pane_origin_x(controller);
    let pane_origin_y = crate::engine::space::screen_center_y() - 62.0;
    let top_y = MACHINE_RECORD_DEFAULT_ROW_HEIGHT * 0.8;
    let score_w = 70.0;
    let score_h = 28.0;
    let score_bg = color::rgba_hex("#101519");
    let score_text = format!("{:.2}", score_info.score_percent * 100.0);

    // SL Pane7: keep a fixed left text column and dedicate the right side to the QR.
    let qr_size = 168.0;
    let qr_left = -26.0;
    let qr_top_y = top_y - 6.0;
    let qr_center_x = qr_left + qr_size * 0.5;
    let qr_center_y = qr_top_y + qr_size * 0.5;
    // SL parity: keep QR fixed and shift the full left info column as a unit.
    let left_col_x = -150.0;
    let score_y = qr_top_y - 6.0;

    let help_zoom = if gs_valid { 0.80 } else { 0.675 };
    let mut children = Vec::with_capacity(10);

    children.push(act!(quad:
        align(0.0, 0.0):
        xy(left_col_x, score_y):
        setsize(score_w, score_h):
        z(101):
        diffuse(score_bg[0], score_bg[1], score_bg[2], 1.0)
    ));
    children.push(act!(text:
        font(current_machine_font_key(FontRole::Header)):
        settext(score_text):
        align(1.0, 0.5):
        xy(left_col_x + 60.0, score_y + 12.0):
        zoom(0.25):
        z(102):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(right)
    ));

    let title_y = top_y + 36.0;
    children.push(act!(text:
        font("miso"):
        settext(GS_QR_TITLE):
        align(0.0, 0.0):
        xy(left_col_x + 4.0, title_y + 1.0):
        zoom(1.0):
        z(101):
        diffuse(1.0, 1.0, 1.0, 1.0)
    ));

    children.push(act!(quad:
        align(0.0, 0.0):
        xy(left_col_x + 4.0, title_y + 23.0):
        setsize(96.0, 1.0):
        z(101):
        diffuse(1.0, 1.0, 1.0, 0.33)
    ));

    children.push(act!(text:
        font("miso"):
        settext(help_text):
        align(0.0, 0.0):
        xy(left_col_x + 1.0, title_y + 31.0):
        zoom(help_zoom):
        maxwidth(98.0 / help_zoom):
        z(101):
        diffuse(1.0, 1.0, 1.0, 1.0)
    ));

    let qr_content = if gs_valid {
        score_info.groovestats.manual_qr_url.as_deref()
    } else {
        Some(GS_QR_INVALID_URL)
    };
    if let Some(content) = qr_content {
        let qr_actors = qr_code::build(qr_code::QrCodeParams {
            content,
            center_x: qr_center_x,
            center_y: qr_center_y,
            size: qr_size,
            border_modules: 1,
            z: 0,
        });
        if qr_actors.is_empty() {
            children.push(act!(text:
                font("miso"):
                settext(GS_QR_FALLBACK_TEXT):
                align(0.5, 0.5):
                xy(qr_center_x, qr_center_y):
                zoom(0.8):
                z(101):
                diffuse(1.0, 0.3, 0.3, 1.0):
                horizalign(center)
            ));
        } else {
            children.extend(qr_actors);
        }
    }

    if !gs_valid {
        for rotation in [45.0_f32, -45.0_f32] {
            children.push(act!(quad:
                align(0.5, 0.5):
                xy(qr_center_x, qr_center_y):
                setsize(qr_size * 1.15, 12.0):
                rotationz(rotation):
                z(102):
                diffuse(0.95, 0.05, 0.05, 0.92)
            ));
        }
    }

    vec![Actor::Frame {
        align: [0.5, 0.5],
        offset: [pane_origin_x, pane_origin_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z: 101,
        children,
    }]
}
