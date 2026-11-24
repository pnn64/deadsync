//! Local profile selection and late-join panels.

use crate::presentation::{ACCENT, INK, Layout, MUTED, quad};
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_height, screen_width};
use deadsync_theme::profile_picker::{
    PROFILE_PICKER_VISIBLE_ROWS, ProfilePickerPanel, ProfilePickerScreenView,
};

const PLAYER_TWO: [f32; 3] = [0.48, 0.94, 0.51];
const PANEL_WIDTH: f32 = 274.0;
const FOCUS_ROW: usize = PROFILE_PICKER_VISIBLE_ROWS / 2;

/// Draw the current picker, either as a screen or over the music wheel.
pub fn push_profile_picker_actors(
    actors: &mut Vec<Actor>,
    view: ProfilePickerScreenView<'_>,
    overlay: bool,
) {
    let (width, height) = (screen_width(), screen_height());
    let layout = Layout::centered(width, height);
    let z = if overlay { 1450 } else { 0 };
    if !overlay {
        actors.push(quad(
            [0.0, 0.0],
            [width, height],
            [0.025, 0.037, 0.064, 1.0],
            -20,
        ));
    }
    let start = actors.len();
    if overlay {
        actors.push(quad(
            [0.0, 0.0],
            [width, height],
            [0.012, 0.019, 0.033, 0.92],
            z,
        ));
    }
    let mut title = layout.text([320.0, 57.0], 0.5, view.title.into(), 1.72, rgba(INK));
    title.z(z + 1);
    actors.push(title.build(0));

    for ((side, x, color), panel) in [("P1", 34.0, ACCENT), ("P2", 332.0, PLAYER_TWO)]
        .into_iter()
        .zip(view.panels)
    {
        let accent = if panel.invalid_choice {
            [1.0, 0.35, 0.27]
        } else {
            color
        };
        actors.push(layout.panel(
            x,
            99.0,
            [PANEL_WIDTH, 282.0],
            rgba([0.061, 0.105, 0.164]),
            z + 1,
        ));
        actors.push(layout.panel(x, 99.0, [PANEL_WIDTH, 3.0], rgba(accent), z + 2));
        let mut header = layout.text([x + 15.0, 124.0], 0.0, side.into(), 1.15, rgba(accent));
        header.z(z + 3);
        actors.push(header.build(0));

        if panel.joined && !panel.ready {
            push_choices(actors, layout, &panel, x, accent, z);
        } else {
            let center = x + PANEL_WIDTH * 0.5;
            let message = if panel.ready {
                if let Some(name) = panel.names[FOCUS_ROW] {
                    let mut label = layout.text([center, 204.0], 0.5, name.into(), 1.16, rgba(INK));
                    label.maxwidth(layout.size([PANEL_WIDTH - 28.0, 0.0])[0]);
                    label.z(z + 3);
                    actors.push(label.build(0));
                }
                view.waiting_text
            } else {
                view.join_text
            };
            let mut status = layout.text([center, 243.0], 0.5, message.into(), 1.0, rgba(accent));
            status.maxwidth(layout.size([PANEL_WIDTH - 28.0, 0.0])[0]);
            status.z(z + 3);
            actors.push(status.build(0));
        }
    }
    let alpha = view.alpha.clamp(0.0, 1.0);
    if alpha < 1.0 {
        for actor in &mut actors[start..] {
            actor.mul_alpha(alpha);
        }
    }
}

fn push_choices(
    actors: &mut Vec<Actor>,
    layout: Layout,
    panel: &ProfilePickerPanel<'_>,
    x: f32,
    accent: [f32; 3],
    z: i16,
) {
    for (row, name) in panel.names.iter().enumerate() {
        let Some(name) = name else { continue };
        let focused = row == FOCUS_ROW;
        let y = 157.0 + row as f32 * 32.0;
        if focused {
            actors.push(layout.panel(
                x + 9.0,
                y - 14.0,
                [PANEL_WIDTH - 18.0, 28.0],
                rgba([0.132, 0.213, 0.303]),
                z + 2,
            ));
            actors.push(layout.panel(x + 9.0, y - 14.0, [3.0, 28.0], rgba(accent), z + 3));
        }
        let mut text = layout.text(
            [x + 20.0, y],
            0.0,
            TextContent::from(*name),
            1.0,
            rgba(if focused { INK } else { MUTED }),
        );
        text.maxwidth(layout.size([PANEL_WIDTH - 40.0, 0.0])[0]);
        text.z(z + 4);
        actors.push(text.build(0));
    }
    actors.push(layout.panel(
        x + 14.0,
        311.0,
        [PANEL_WIDTH - 28.0, 1.0],
        rgba([0.24, 0.35, 0.46]),
        z + 2,
    ));
    if let Some(avatar) = panel.avatar {
        actors.push(layout.image([x + 38.0, 345.0], [46.0, 46.0], avatar, z + 3));
    }
    if let Some(songs_played) = panel.songs_played {
        let text_x = if panel.avatar.is_some() {
            x + 74.0
        } else {
            x + 16.0
        };
        let mut text = layout.text([text_x, 345.0], 0.0, songs_played.into(), 0.90, rgba(MUTED));
        text.maxwidth(layout.size([x + PANEL_WIDTH - 14.0 - text_x, 0.0])[0]);
        text.z(z + 3);
        actors.push(text.build(0));
    }
}

const fn rgba([r, g, b]: [f32; 3]) -> [f32; 4] {
    [r, g, b, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::space::{Metrics, with_current_metrics};
    use std::sync::Arc;

    #[test]
    fn late_join_keeps_first_profile_ready_and_draws_second_profile_over_the_wheel() {
        let title = Arc::<str>::from("Select Profile");
        let join = Arc::<str>::from("Press Start");
        let waiting = Arc::<str>::from("Waiting");
        let names = [Arc::<str>::from("First"), Arc::<str>::from("Second")];
        let panels = std::array::from_fn(|player| {
            let mut rows = [None; PROFILE_PICKER_VISIBLE_ROWS];
            rows[FOCUS_ROW] = Some(&names[player]);
            ProfilePickerPanel {
                joined: true,
                ready: player == 0,
                names: rows,
                avatar: None,
                songs_played: None,
                invalid_choice: false,
            }
        });
        let mut actors = Vec::new();
        with_current_metrics(Metrics::centered(854.0, 480.0), || {
            push_profile_picker_actors(
                &mut actors,
                ProfilePickerScreenView {
                    title: &title,
                    join_text: &join,
                    waiting_text: &waiting,
                    panels,
                    alpha: 1.0,
                },
                true,
            );
        });
        let text = |name: &str| {
            actors.iter().find_map(|actor| match actor {
                Actor::Text {
                    content, offset, z, ..
                } if content.as_str() == name => Some((*offset, *z)),
                _ => None,
            })
        };
        assert_eq!(text("First"), Some(([278.0, 204.0], 1453)));
        assert_eq!(text("Waiting"), Some(([278.0, 243.0], 1453)));
        assert_eq!(text("Second"), Some(([459.0, 221.0], 1454)));
        assert!(text("Press Start").is_none());
    }
}
