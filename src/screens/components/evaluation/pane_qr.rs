use crate::act;
use crate::game::profile;
use crate::screens::components::qr_code;
use crate::screens::evaluation::ScoreInfo;
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;

use super::utils::pane_origin_x;

const MACHINE_RECORD_DEFAULT_ROW_HEIGHT: f32 = 22.0;
const GS_QR_URL: &str = "https://www.groovestats.com";
const GS_QR_TITLE: &str = "GrooveStats QR";
const GS_QR_HELP_TEXT: &str =
    "Scan with your phone\nto upload this score\nto your GrooveStats\naccount.";
const GS_QR_FALLBACK_TEXT: &str = "QR Unavailable";

pub fn build_gs_qr_pane(score_info: &ScoreInfo, controller: profile::PlayerSide) -> Vec<Actor> {
    let pane_origin_x = pane_origin_x(controller);
    let pane_origin_y = crate::core::space::screen_center_y() - 62.0;
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

    let mut children = Vec::with_capacity(8);

    children.push(act!(quad:
        align(0.0, 0.0):
        xy(left_col_x, score_y):
        setsize(score_w, score_h):
        z(101):
        diffuse(score_bg[0], score_bg[1], score_bg[2], 1.0)
    ));
    children.push(act!(text:
        font("wendy_white"):
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
        settext(GS_QR_HELP_TEXT):
        align(0.0, 0.0):
        xy(left_col_x + 1.0, title_y + 31.0):
        zoom(0.80):
        z(101):
        diffuse(1.0, 1.0, 1.0, 1.0)
    ));

    let qr_actors = qr_code::build(qr_code::QrCodeParams {
        content: GS_QR_URL,
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

    vec![Actor::Frame {
        align: [0.5, 0.5],
        offset: [pane_origin_x, pane_origin_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z: 101,
        children,
    }]
}
