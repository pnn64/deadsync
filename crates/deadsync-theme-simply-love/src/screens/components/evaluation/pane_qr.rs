use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::screens::components::shared::qr_code;
use crate::screens::evaluation::ScoreInfo;
use deadlib_present::actors::{Actor, SizeSpec, TextContent};
use deadlib_present::color;
use deadsync_profile as profile_data;

use super::utils::pane_origin_x;

const MACHINE_RECORD_DEFAULT_ROW_HEIGHT: f32 = 22.0;
const GS_QR_INVALID_URL: &str = "https://www.youtube.com/watch?v=FMABVVk4Ge4";
const GS_QR_TITLE: &str = "GrooveStats QR";
const GS_QR_HELP_TEXT_VALID: &str =
    "Scan with your phone\nto upload this score\nto your GrooveStats\naccount.";
const GS_QR_FALLBACK_TEXT: &str = "QR Unavailable";
const GS_QR_PANE_Z: i16 = 101;
const GS_QR_SIZE: f32 = 168.0;

fn qr_fallback(center_x: f32, center_y: f32) -> Actor {
    act!(text:
        font("miso"):
        settext(GS_QR_FALLBACK_TEXT):
        align(0.5, 0.5):
        xy(center_x, center_y):
        zoom(0.8):
        z(GS_QR_PANE_Z):
        diffuse(1.0, 0.3, 0.3, 1.0):
        horizalign(center)
    )
}

fn push_qr(
    children: &mut Vec<Actor>,
    qr: Option<&qr_code::PreparedQrCode>,
    center_x: f32,
    center_y: f32,
) {
    let Some(qr) = qr else {
        children.push(qr_fallback(center_x, center_y));
        return;
    };
    qr.push(children, center_x, center_y, 1, GS_QR_PANE_Z);
}

/// Immutable score text, help copy, and QR geometry retained by Evaluation.
///
/// Initialization performs all formatting, joining, and QR encoding. Actor
/// frames only clone bounded/shared payloads and the prepared geometry `Arc`.
#[derive(Clone)]
pub(crate) struct QrPanePresentation {
    score: TextContent,
    help: TextContent,
    qr: Option<qr_code::PreparedQrCode>,
    valid: bool,
}

impl QrPanePresentation {
    pub(crate) fn new(score: &ScoreInfo) -> Self {
        Self::from_parts(
            score.score_percent,
            score.groovestats.valid,
            &score.groovestats.reason_lines,
            score.groovestats.manual_qr_url.as_deref(),
        )
    }

    fn from_parts(
        score_percent: f64,
        valid: bool,
        reason_lines: &[String],
        manual_qr_url: Option<&str>,
    ) -> Self {
        let help = if valid {
            TextContent::static_str(GS_QR_HELP_TEXT_VALID)
        } else if reason_lines.is_empty() {
            TextContent::static_str("This score is invalid for GrooveStats.")
        } else if reason_lines.len() == 1 {
            super::retained_str(&reason_lines[0])
        } else {
            super::retained_str(&reason_lines.join("\n"))
        };
        let qr_content = if valid {
            manual_qr_url
        } else {
            Some(GS_QR_INVALID_URL)
        };
        Self {
            score: super::retained_text(format_args!("{:.2}", score_percent * 100.0)),
            help,
            qr: qr_content.and_then(|content| qr_code::prepare(content, GS_QR_SIZE)),
            valid,
        }
    }
}

pub(crate) fn build_gs_qr_pane(
    presentation: &QrPanePresentation,
    controller: profile_data::PlayerSide,
    machine_font: MachineFont,
) -> Vec<Actor> {
    let pane_origin_x = pane_origin_x(controller);
    let pane_origin_y = deadlib_present::space::screen_center_y() - 62.0;
    let top_y = MACHINE_RECORD_DEFAULT_ROW_HEIGHT * 0.8;
    let score_w = 70.0;
    let score_h = 28.0;
    let score_bg = color::rgba_hex("#101519");

    // SL Pane7: keep a fixed left text column and dedicate the right side to the QR.
    let qr_size = GS_QR_SIZE;
    let qr_left = -26.0;
    let qr_top_y = top_y - 6.0;
    let qr_center_x = qr_left + qr_size * 0.5;
    let qr_center_y = qr_top_y + qr_size * 0.5;
    // SL parity: keep QR fixed and shift the full left info column as a unit.
    let left_col_x = -150.0;
    let score_y = qr_top_y - 6.0;

    let help_zoom = if presentation.valid { 0.80 } else { 0.675 };
    let mut children = Vec::with_capacity(10);

    children.push(act!(quad:
        align(0.0, 0.0):
        xy(left_col_x, score_y):
        setsize(score_w, score_h):
        z(101):
        diffuse(score_bg[0], score_bg[1], score_bg[2], 1.0)
    ));
    children.push(act!(text:
        font(machine_font_key(machine_font, FontRole::Header)):
        settext(presentation.score.clone()):
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
        settext(presentation.help.clone()):
        align(0.0, 0.0):
        xy(left_col_x + 1.0, title_y + 31.0):
        zoom(help_zoom):
        maxwidth(98.0 / help_zoom):
        z(101):
        diffuse(1.0, 1.0, 1.0, 1.0)
    ));

    push_qr(
        &mut children,
        presentation.qr.as_ref(),
        qr_center_x,
        qr_center_y,
    );

    if !presentation.valid {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_uses_the_pane_content_layer() {
        let mut actors = Vec::new();
        let qr = qr_code::prepare("https://example.com/QR/score", GS_QR_SIZE)
            .expect("QR should prepare");
        push_qr(&mut actors, Some(&qr), 0.0, 0.0);

        let [Actor::Frame { z, children, .. }] = actors.as_slice() else {
            panic!("expected QR frame");
        };
        assert_eq!(*z, GS_QR_PANE_Z);
        assert!(matches!(
            children.as_slice(),
            [Actor::Sprite { z: 0, .. }, Actor::Mesh { z: 1, .. }]
        ));
    }

    #[test]
    fn missing_qr_payload_is_visible_instead_of_blank() {
        let mut actors = Vec::new();
        push_qr(&mut actors, None, 0.0, 0.0);

        let [Actor::Text { content, z, .. }] = actors.as_slice() else {
            panic!("expected QR fallback text");
        };
        assert_eq!(content.as_str(), GS_QR_FALLBACK_TEXT);
        assert_eq!(*z, GS_QR_PANE_Z);
    }

    #[test]
    fn qr_presentation_retains_score_help_and_geometry() {
        let presentation = QrPanePresentation::from_parts(
            0.98765,
            true,
            &[],
            Some("https://example.com/QR/score"),
        );

        assert_eq!(presentation.score.as_str(), "98.77");
        assert_eq!(presentation.help.as_str(), GS_QR_HELP_TEXT_VALID);
        assert!(presentation.qr.is_some());
        assert!(presentation.valid);
    }

    #[test]
    fn qr_presentation_joins_invalid_reasons_once() {
        let reasons = vec!["Autoplay was used".into(), "Chart is unsupported".into()];
        let presentation = QrPanePresentation::from_parts(0.5, false, &reasons, None);
        let clone = presentation.help.clone();

        assert_eq!(
            presentation.help.as_str(),
            "Autoplay was used\nChart is unsupported"
        );
        let (TextContent::Shared(source), TextContent::Shared(clone)) =
            (&presentation.help, &clone)
        else {
            panic!("joined reasons should use shared text");
        };
        assert!(std::sync::Arc::ptr_eq(source, clone));
        assert!(presentation.qr.is_some());
        assert!(!presentation.valid);
    }
}
