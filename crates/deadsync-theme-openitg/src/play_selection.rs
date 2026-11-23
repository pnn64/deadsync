//! Style and mode carousels for the existing DeadSync session flow.

use crate::presentation::{ACCENT, HEIGHT, INK, Layout, MUTED, WIDTH, quad};
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_height, screen_width};
use deadsync_theme::views::{PlaySelectionKind, PlaySelectionView};
use std::f32::consts::TAU;
use std::sync::Arc;

const ITEM_SECONDS: f32 = 0.3;
const PLAYER_TWO: [f32; 3] = [0.48, 0.94, 0.51];

/// Present the pending catalog load using the shared screen's elapsed time.
pub fn push_profile_load_actors(actors: &mut Vec<Actor>, label: &Arc<str>, elapsed_seconds: f32) {
    let (width, height) = (screen_width(), screen_height());
    let layout = Layout::centered(width, height);
    actors.push(quad(
        [0.0, 0.0],
        [width, height],
        [0.025, 0.037, 0.064, 1.0],
        -20,
    ));
    actors.push(layout.bounded_label([320.0, 231.0], 0.5, 384.0, label.into(), 1.48, rgba(INK)));
    actors.push(layout.panel(200.0, 262.0, [240.0, 3.0], [0.17, 0.26, 0.38, 1.0], 1));
    let phase = elapsed_seconds.rem_euclid(0.9) / 0.9;
    let sweep = 1.0 - (2.0 * phase - 1.0).abs();
    actors.push(layout.panel(200.0 + 180.0 * sweep, 262.0, [60.0, 3.0], rgba(ACCENT), 2));
}

/// Visual scroll position for the active style or mode screen.
#[derive(Default)]
pub struct PlaySelectionCarousel {
    count: usize,
    selected: usize,
    from: f32,
    started_at: f64,
}

impl PlaySelectionCarousel {
    fn position_at(&self, time: f64) -> f32 {
        let target = self.selected as f32;
        let duration = f64::from((target - self.from).abs() * ITEM_SECONDS);
        if duration == 0.0 {
            return target;
        }
        let progress = ((time - self.started_at) / duration).clamp(0.0, 1.0) as f32;
        (target - self.from).mul_add(progress, self.from)
    }

    fn select_at(&mut self, count: usize, selected: usize, time: f64) -> f32 {
        if self.count != count {
            *self = Self {
                count,
                selected,
                from: selected as f32,
                started_at: time,
            };
        } else if self.selected != selected {
            let previous = self.selected as f32;
            let mut step = (selected as f32 - previous).rem_euclid(count as f32);
            if step > count as f32 / 2.0 {
                step -= count as f32;
            }
            self.from = selected as f32 + (self.position_at(time) - previous) - step;
            self.selected = selected;
            self.started_at = time;
        }
        self.position_at(time)
    }

    /// Draw the active selection from the shared controller's current view.
    pub fn push_actors(&mut self, actors: &mut Vec<Actor>, view: PlaySelectionView<'_>, time: f64) {
        let count = view.choices.len();
        let Some(selected) = view.choices.get(view.selected) else {
            return;
        };
        let center = self.select_at(count, view.selected, time);
        let (width, height) = (screen_width(), screen_height());
        let layout = Layout::centered(width, height);
        actors.push(quad(
            [0.0, 0.0],
            [width, height],
            [0.025, 0.037, 0.064, 1.0],
            -20,
        ));
        actors.push(layout.panel(0.0, 0.0, [WIDTH, HEIGHT], [0.047, 0.075, 0.119, 1.0], -19));
        actors.push(layout.panel(32.0, 80.0, [576.0, 1.0], [0.24, 0.35, 0.46, 1.0], -18));
        actors.push(layout.label([32.0, 49.0], 0.0, view.title.into(), 1.78, rgba(INK)));

        // The far side of the ring is dimmer and drawn behind the near side.
        let [base_y, depth_scale, dim] = match view.kind {
            PlaySelectionKind::Style { .. } => [232.0, 1.0 / 2.2, 0.15],
            PlaySelectionKind::Mode => [240.0, 1.0 / 2.8, 0.30],
        };
        let mut cards = [Card::default(); 3];
        debug_assert!(count <= cards.len());
        let cards = &mut cards[..count];
        for (index, card) in cards.iter_mut().enumerate() {
            let (side, depth) = ((index as f32 - center) * TAU / count as f32).sin_cos();
            let near = depth.max(0.0);
            *card = Card {
                index,
                x: side.mul_add(200.0, 320.0),
                y: (depth * 200.0).mul_add(depth_scale, base_y),
                zoom: near.mul_add(0.3, 0.7),
                light: near.mul_add(1.0 - dim, dim),
                depth,
            };
        }
        cards.sort_unstable_by(|a, b| a.depth.total_cmp(&b.depth));
        for (layer, card) in cards.iter().enumerate() {
            card.push(actors, layout, &view, 4 + layer as i16 * 4);
        }

        actors.push(layout.panel(100.0, 369.0, [440.0, 59.0], [0.018, 0.029, 0.054, 1.0], 1));
        actors.push(layout.panel(100.0, 369.0, [440.0, 1.0], rgba(ACCENT), 2));
        let description = selected.description.map_or_else(
            || {
                TextContent::Static(
                    [
                        "One player uses one pad.",
                        "Each player uses a pad.",
                        "One player uses both pads.",
                    ][view.selected],
                )
            },
            TextContent::from,
        );
        actors.push(layout.paragraph([114.0, 382.0], [412.0, 38.0], description, 0.93, rgba(INK)));
        push_players(actors, layout, &view);

        let exit = view.exit_progress.clamp(0.0, 1.0);
        if exit > 0.0 {
            actors.push(quad(
                [0.0, 0.0],
                [width, height],
                [0.0, 0.0, 0.0, exit],
                100,
            ));
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Card {
    index: usize,
    x: f32,
    y: f32,
    zoom: f32,
    light: f32,
    depth: f32,
}

impl Card {
    fn push(&self, actors: &mut Vec<Actor>, layout: Layout, view: &PlaySelectionView<'_>, z: i16) {
        let Self {
            index,
            x,
            y,
            zoom,
            light,
            ..
        } = *self;
        let selected = index == view.selected;
        let tint = |color: [f32; 3]| rgba(color.map(|channel| channel * light));
        let edge = if selected { ACCENT } else { [0.47, 0.59, 0.74] };
        let left = x - 77.0 * zoom;
        let top = y - 41.0 * zoom;
        actors.push(layout.panel(left, top, [154.0 * zoom, 82.0 * zoom], tint(edge), z));
        actors.push(layout.panel(
            left + 1.0,
            top + 1.0,
            [154.0 * zoom - 2.0, 82.0 * zoom - 2.0],
            tint([0.088, 0.151, 0.235]),
            z + 1,
        ));
        match view.kind {
            PlaySelectionKind::Style { pump } => {
                let pad_y = y - 10.0 * zoom;
                let shift = 20.0 * zoom;
                let left_pad = if index == 0 { x } else { x - shift };
                push_pad(
                    actors,
                    layout,
                    [left_pad, pad_y],
                    zoom,
                    pump,
                    tint(ACCENT),
                    z + 2,
                );
                if index != 0 {
                    let second = if index == 1 { PLAYER_TWO } else { ACCENT };
                    push_pad(
                        actors,
                        layout,
                        [x + shift, pad_y],
                        zoom,
                        pump,
                        tint(second),
                        z + 2,
                    );
                }
            }
            PlaySelectionKind::Mode => {
                let bars = [3, 5, 4][index];
                let left = x - (bars as f32 * 9.0 - 3.0) * zoom / 2.0;
                for bar in 0..bars {
                    let height = (10.0 + 5.0 * bar as f32) * zoom;
                    actors.push(layout.panel(
                        left + bar as f32 * 9.0 * zoom,
                        y + 5.0 * zoom - height,
                        [6.0 * zoom, height],
                        tint(ACCENT),
                        z + 2,
                    ));
                }
            }
        }
        let mut label = layout.text(
            [x, y + 24.0 * zoom],
            0.5,
            view.choices[index].label.into(),
            zoom,
            rgba(INK.map(|channel| channel * light.max(0.55))),
        );
        label.maxwidth(layout.size([140.0 * zoom, 0.0])[0]);
        label.z(z + 3);
        actors.push(label.build(0));
    }
}

fn push_pad(
    actors: &mut Vec<Actor>,
    layout: Layout,
    [x, y]: [f32; 2],
    zoom: f32,
    pump: bool,
    color: [f32; 4],
    z: i16,
) {
    let pitch = 10.0 * zoom;
    let active = if pump {
        [0, 2, 4, 6, 8].as_slice()
    } else {
        [1, 3, 5, 7].as_slice()
    };
    actors.push(layout.panel(
        x - 1.6 * pitch,
        y - 1.6 * pitch,
        [3.2 * pitch; 2],
        [0.022, 0.032, 0.051, 1.0],
        z,
    ));
    for &tile in active {
        let dx = tile % 3;
        let dy = tile / 3;
        actors.push(layout.panel(
            x + (dx as f32 - 1.42) * pitch,
            y + (dy as f32 - 1.42) * pitch,
            [0.84 * pitch; 2],
            color,
            z + 1,
        ));
    }
}

fn push_players(actors: &mut Vec<Actor>, layout: Layout, view: &PlaySelectionView<'_>) {
    actors.push(layout.panel(0.0, 442.0, [WIDTH, 38.0], [0.018, 0.029, 0.054, 1.0], 0));
    for ((name, x, color), joined) in [("P1", 32.0, ACCENT), ("P2", 344.0, PLAYER_TWO)]
        .into_iter()
        .zip(view.player_names)
    {
        let status = match joined {
            Some(display_name) => TextContent::inline_str(display_name)
                .unwrap_or_else(|| TextContent::Owned(display_name.to_owned())),
            None => view.join_prompt.into(),
        };
        actors.push(layout.label([x, 461.0], 0.0, name.into(), 0.94, rgba(color)));
        actors.push(layout.bounded_label(
            [x + 34.0, 461.0],
            0.0,
            225.0,
            status,
            0.94,
            rgba(if joined.is_some() { INK } else { MUTED }),
        ));
    }
}

const fn rgba([r, g, b]: [f32; 3]) -> [f32; 4] {
    [r, g, b, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::space::{Metrics, with_current_metrics};
    use deadsync_theme::views::PlaySelectionChoice;
    use std::sync::Arc;

    #[test]
    fn carousel_wraps_one_slot_and_retargets_from_its_visible_position() {
        let mut carousel = PlaySelectionCarousel::default();
        assert_eq!(carousel.select_at(3, 2, 0.0), 2.0);
        assert_eq!(carousel.select_at(3, 0, 1.0), -1.0);
        assert!((carousel.select_at(3, 0, 1.15) + 0.5).abs() < 0.0001);
        let previous = carousel.select_at(3, 0, 1.2);
        let reversed = carousel.select_at(3, 2, 1.2);
        assert!((reversed - previous - 3.0).abs() < 0.0001);
        assert_eq!(carousel.select_at(3, 2, 1.4), 2.0);
        assert_eq!(carousel.select_at(2, 0, 2.0), 0.0);
    }

    #[test]
    fn mode_draws_only_available_choices_with_a_centered_carousel_and_join_prompt() {
        let mut carousel = PlaySelectionCarousel::default();
        let labels = ["Regular", "Marathon", "Premium Free"].map(Arc::<str>::from);
        let description = Arc::<str>::from("Play a regular session.");
        let title = Arc::<str>::from("Select Play Mode");
        let join_prompt = Arc::<str>::from("Press Start");
        let choices = labels.each_ref().map(|label| PlaySelectionChoice {
            label,
            description: Some(&description),
        });
        let mut actors = Vec::new();
        for (width, count) in [(640.0, 2), (854.0, 3)] {
            actors.clear();
            with_current_metrics(Metrics::centered(width, 480.0), || {
                carousel.push_actors(
                    &mut actors,
                    PlaySelectionView {
                        kind: PlaySelectionKind::Mode,
                        title: &title,
                        choices: &choices[..count],
                        selected: 0,
                        player_names: [Some("Player One"), None],
                        join_prompt: &join_prompt,
                        exit_progress: 0.0,
                    },
                    0.0,
                );
            });
            let text = |value: &str| {
                actors.iter().find_map(|actor| match actor {
                    Actor::Text {
                        content, offset, ..
                    } if content.as_str() == value => Some(*offset),
                    _ => None,
                })
            };
            assert_eq!(text("Regular").unwrap()[0], width / 2.0);
            assert_eq!(text("Premium Free").is_some(), count == 3);
            assert!(text("Player One").unwrap()[0] < width / 2.0);
            assert!(text("Press Start").unwrap()[0] > width / 2.0);
        }
    }
}
