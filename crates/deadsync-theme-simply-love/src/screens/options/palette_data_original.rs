// Frozen from 8711fc76304beb7ec26866e1a7a99e5647469e64; benchmark and regression reference only.
use super::*;

pub(super) fn push_editor(
    out: &mut Vec<Actor>,
    state: &State,
    palette_id: &str,
    selected: usize,
    channel_focus: Option<usize>,
    editing_name: bool,
    name_buffer: &str,
    blink_t: f32,
    message: Option<&str>,
    accent: [f32; 4],
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    let Some(entry) = state.judgment_palettes.palette(palette_id) else {
        return;
    };
    for row in 0..=EDITOR_DONE_ROW {
        let y = (row as f32).mul_add(34.0, cy - 140.0);
        if row == selected {
            out.push(act!(quad:
                align(0.5, 0.5): xy(cx, y): zoomto(PANEL_W - 30.0, 31.0):
                diffuse(accent[0], accent[1], accent[2], 0.72): z(Z + 4)
            ));
        }
        if row == 0 {
            let cursor = if editing_name && blink_t < CURSOR_PERIOD * 0.5 {
                "_"
            } else {
                ""
            };
            let value = if editing_name {
                format!("{name_buffer}{cursor}")
            } else {
                entry.name.clone()
            };
            push_editor_text(
                out,
                tr("JudgmentPalettes", "Name").to_string(),
                value,
                y,
                cx,
                bold_font,
            );
        } else if row <= 7 {
            let role = JudgmentColorRole::ALL[row - 1];
            let rgba = entry.palette.color(role);
            let color = Color::from_rgba(rgba);
            push_editor_text(
                out,
                tr("JudgmentPalettes", role.config_key()).to_string(),
                color.to_hex(),
                y,
                cx,
                bold_font,
            );
            out.push(act!(quad:
                align(0.5, 0.5): xy(cx + 95.0, y): zoomto(54.0, 23.0):
                diffuse(rgba[0], rgba[1], rgba[2], 1.0): z(Z + 6)
            ));
            let channels = [color.r, color.g, color.b].map(|v| (v * 255.0).round() as u8);
            let channel_text = ["R", "G", "B"]
                .into_iter()
                .enumerate()
                .map(|(index, name)| {
                    if row == selected && channel_focus == Some(index) {
                        format!("[{name} {:03}]", channels[index])
                    } else {
                        format!("{name} {:03}", channels[index])
                    }
                })
                .collect::<Vec<_>>()
                .join("  ");
            out.push(act!(text:
                font("miso"): settext(channel_text): align(1.0, 0.5):
                xy(PANEL_W.mul_add(0.5, cx) - 26.0, y): zoom(0.62):
                diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(right)
            ));
        } else {
            push_editor_text(
                out,
                tr("JudgmentPalettes", "Done").to_string(),
                String::new(),
                y,
                cx,
                bold_font,
            );
        }
    }
    let help = message.map(ToOwned::to_owned).unwrap_or_else(|| {
        if editing_name {
            tr("JudgmentPalettes", "NameHelp").to_string()
        } else if channel_focus.is_some() {
            tr("JudgmentPalettes", "ColorHelp").to_string()
        } else {
            tr("JudgmentPalettes", "EditorHelp").to_string()
        }
    });
    push_footer(out, help, accent, cx, cy);
}
