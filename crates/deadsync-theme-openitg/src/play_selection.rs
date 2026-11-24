//! Style and mode carousels for the existing DeadSync session flow.

use crate::presentation::{
    HEIGHT, Layout, italic, push_red_backdrop, push_red_streaks, quad, sprite,
};
use crate::resources::{MENU_HEADING_FONT, MENU_NORMAL_FONT, TITLE_FONT};
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::anim::bounceend_p;
use deadlib_present::space::{screen_height, screen_width};
use deadsync_theme::views::{PlaySelectionKind, PlaySelectionView};
use std::f32::consts::TAU;
use std::sync::Arc;

const ITEM_SECONDS: f32 = 0.3;
const WHITE: [f32; 4] = [1.0; 4];
const RADIUS: f32 = 200.0;
const STYLE_FOCAL_LENGTH: f32 = 415.6922; // 60-degree FOV at a logical height of 480 px.
const MODE_FOCAL_LENGTH: f32 = 240.0; // 90-degree FOV.
const PAD_NEUTRAL_POSE: u32 = 12;
const PAD_ATLAS_COLS: u32 = 4;
const PAD_ATLAS_ROWS: u32 = 7;
// The atlas cell includes space for the outer edge at the ends of the wag.
const PAD_ATLAS_SCALE: [f32; 2] = [476.0 / 422.0, 222.0 / 210.0];

fn pad_pose(elapsed: f64) -> u32 {
    let phase = elapsed.rem_euclid(1.0) as f32;
    ((1.0 + (phase * TAU).sin()) * PAD_NEUTRAL_POSE as f32).round() as u32
}

/// Present the pending catalog load using the shared screen's elapsed time.
pub fn push_profile_load_actors(actors: &mut Vec<Actor>, label: &Arc<str>, elapsed_seconds: f32) {
    let (width, height) = (screen_width(), screen_height());
    let layout = Layout::centered(width, height);
    crate::evaluation::push_arcade_backdrop(actors, [width, height], elapsed_seconds);
    let mut heading = layout.text([320.0, 240.0], 0.5, label.into(), 1.0, WHITE);
    heading.font(MENU_HEADING_FONT);
    heading.maxwidth(layout.size([500.0, 0.0])[0]);
    heading.shadowlength(layout.size([4.0, 0.0])[0]);
    actors.push(italic(heading.build(0), -0.16));
}

/// Visual scroll position for the active style or mode screen.
#[derive(Default)]
pub struct PlaySelectionCarousel {
    count: usize,
    selected: usize,
    from: f32,
    started_at: f64,
    entered_at: f64,
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
                entered_at: time,
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
        let elapsed = (time - self.entered_at).max(0.0) as f32;
        let style = matches!(view.kind, PlaySelectionKind::Style { .. });
        let focused_pad_pose = if matches!(view.kind, PlaySelectionKind::Style { pump: false }) {
            pad_pose((time - self.started_at).max(0.0))
        } else {
            PAD_NEUTRAL_POSE
        };
        push_red_backdrop(actors, [width, height], elapsed, -24);
        push_red_streaks(actors, [width, height], elapsed.min(1.0), -22);
        push_selection_header(actors, layout, view.title, elapsed);

        let mut cards = [Card::default(); 3];
        debug_assert!(count <= cards.len());
        let cards = &mut cards[..count];
        for (index, card) in cards.iter_mut().enumerate() {
            *card = Card::new(index, (index as f32 - center) * TAU / count as f32, style);
        }
        cards.sort_unstable_by(|a, b| a.depth.total_cmp(&b.depth));
        for (layer, card) in cards.iter().enumerate() {
            // The actor at z=-100 stands between the front and rear of the ring.
            let z = (if card.depth < 0.5 { 2 } else { 14 }) + layer as i16 * 2;
            card.push(actors, layout, &view, z, focused_pad_pose);
        }
        push_character(actors, layout, style, elapsed);
        let description = selected.description.map_or_else(
            || style_description(view.kind, view.selected).into(),
            TextContent::from,
        );
        push_explanation(actors, layout, description, elapsed, style);
        push_players(actors, layout, &view);
        self.push_arrows(actors, [width, height], time, elapsed, view.exit_progress);

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

const fn style_description(kind: PlaySelectionKind, index: usize) -> &'static str {
    match kind {
        PlaySelectionKind::Style { pump: true } => [
            "One player uses 5 panels",
            "Two players use 10 panels",
            "One player uses both pads",
        ][index],
        _ => [
            "One Player uses 4 Panels",
            "Two Players use 8 Panels",
            "One Player uses 8 Panels",
        ][index],
    }
}

fn push_selection_header(actors: &mut Vec<Actor>, layout: Layout, title: &Arc<str>, elapsed: f32) {
    let content = if title.eq_ignore_ascii_case("select style") {
        "PICK A STYLE".into()
    } else if title.eq_ignore_ascii_case("select mode") {
        "PICK A GAME".into()
    } else {
        title.into()
    };
    let phase = bounceend_p((elapsed / 0.3).clamp(0.0, 1.0));
    let mut text = layout.text([32.0, 39.0], 0.0, content, 1.0, WHITE);
    text.font(MENU_HEADING_FONT);
    text.zoomx(layout.size([phase, 0.0])[0]);
    text.zoomy(layout.size([0.0, (6.0 - 5.0 * phase).max(0.0)])[1]);
    text.shadowlength(layout.size([4.0, 0.0])[0]);
    text.z(32);
    actors.push(italic(text.build(0), -0.16));
}

fn push_character(actors: &mut Vec<Actor>, layout: Layout, style: bool, elapsed: f32) {
    let (key, position, size) = if style {
        (
            "openitg/selection-style-character.png",
            [384.5, 288.4],
            [365.0, 524.0],
        )
    } else {
        (
            "openitg/selection-mode-character.png",
            [320.0, 275.3],
            [345.0, 476.0],
        )
    };
    let mut character = sprite(
        key,
        layout.position(position[0], position[1]),
        layout.size(size),
        [1.0, 1.0, 1.0, (elapsed / 0.3).clamp(0.0, 1.0)],
        10,
    );
    let glow = 1.0 - ((elapsed - 0.3) / 0.3).abs().min(1.0);
    character.glow([1.0, 1.0, 1.0, glow]);
    actors.push(character.build(0));
}

fn push_explanation(
    actors: &mut Vec<Actor>,
    layout: Layout,
    description: TextContent,
    elapsed: f32,
    style: bool,
) {
    let alpha = (elapsed / 0.5).clamp(0.0, 1.0);
    actors.push(
        sprite(
            "openitg/selection-explanation.png",
            layout.position(402.0, 374.0),
            layout.size([356.0, 58.0]),
            [1.0, 1.0, 1.0, alpha],
            24,
        )
        .build(0),
    );
    let zoom = if style { 1.0 } else { 0.72 };
    let mut text = layout.text(
        [240.0, 374.0],
        0.0,
        description,
        zoom,
        [1.0, 1.0, 1.0, alpha],
    );
    text.font(MENU_NORMAL_FONT);
    if !style {
        text.wrapwidthpixels(326.0 / zoom);
    }
    text.maxheight(layout.size([0.0, 47.0])[1]);
    text.maxwidth(layout.size([326.0, 0.0])[0]);
    text.shadowlength(layout.size([2.0, 0.0])[0]);
    text.z(25);
    actors.push(italic(text.build(0), -0.21));
}

impl PlaySelectionCarousel {
    fn push_arrows(
        &self,
        actors: &mut Vec<Actor>,
        [width, height]: [f32; 2],
        time: f64,
        elapsed: f32,
        exiting: f32,
    ) {
        let scale = height / HEIGHT;
        let entry = (1.0 - elapsed / 0.5).clamp(0.0, 1.0);
        let shift = RADIUS * (entry * entry + exiting * exiting) * scale;
        let delta = self.selected as f32 - self.from;
        let blink = (1.0 - ((time - self.started_at) / 0.2) as f32).clamp(0.0, 1.0);
        for (x, direction, rotation) in [
            (64.0 * scale - shift, -1.0, 0.0),
            (width - 64.0 * scale + shift, 1.0, 180.0),
        ] {
            let pulse = if delta * direction > 0.0 { blink } else { 0.0 };
            let size = [58.0, 98.0].map(|v| v * scale * (1.0 + 0.2 * pulse));
            let mut arrow = sprite(
                "openitg/selection-cursor.png",
                [x, height / 2.0],
                size,
                WHITE,
                22,
            );
            arrow.rotationz(rotation);
            arrow.glow([1.0, 1.0, 1.0, pulse * 0.65]);
            actors.push(arrow.build(0));
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
    fn new(index: usize, angle: f32, style: bool) -> Self {
        let (side, depth) = angle.sin_cos();
        let (focal, y, divisor, dim) = if style {
            (STYLE_FOCAL_LENGTH, -8.0, 2.2, 0.15)
        } else {
            (MODE_FOCAL_LENGTH, 0.0, 2.8, 0.30)
        };
        let perspective = focal / (focal + RADIUS * (1.0 - depth));
        let focus = (depth * 2.0 - 1.0).clamp(0.0, 1.0);
        Self {
            index,
            x: 320.0 + side * RADIUS * perspective,
            y: 240.0 + (y + depth * RADIUS / divisor) * perspective,
            zoom: (0.7 + 0.3 * focus) * perspective,
            light: dim + (1.0 - dim) * focus,
            depth,
        }
    }

    fn push(
        &self,
        actors: &mut Vec<Actor>,
        layout: Layout,
        view: &PlaySelectionView<'_>,
        z: i16,
        focused_pad_pose: u32,
    ) {
        let light = self.light;
        let tint = [light, light, light, 1.0];
        let image = |key, x, y, size: [f32; 2], layer| {
            sprite(
                key,
                layout.position(x, y),
                layout.size(size.map(|v| v * self.zoom)),
                tint,
                z + layer,
            )
        };
        let label = view.choices[self.index].label;
        let art = match view.kind {
            PlaySelectionKind::Style { pump } => {
                let pad_y = self.y + 31.0 * self.zoom;
                let (shift, size) = [
                    (0.0, [224.0, 118.0]),
                    (80.0, [168.0, 88.5]),
                    (77.0, [196.0, 103.25]),
                ][self.index];
                let pad_light = 0.5 + 0.5 * light;
                let pad_size = layout.size([
                    size[0] * self.zoom * PAD_ATLAS_SCALE[0],
                    size[1] * self.zoom * PAD_ATLAS_SCALE[1],
                ]);
                for offset in [-shift, shift]
                    .into_iter()
                    .take(if shift > 0.0 { 2 } else { 1 })
                {
                    let x = self.x + offset * self.zoom;
                    if pump {
                        push_pump_pad(actors, layout, [x, pad_y], self.zoom, tint, z);
                    } else {
                        let pose = if self.index == view.selected {
                            focused_pad_pose
                        } else {
                            PAD_NEUTRAL_POSE
                        };
                        let u = (pose % PAD_ATLAS_COLS) as f32 / PAD_ATLAS_COLS as f32;
                        let v = (pose / PAD_ATLAS_COLS) as f32 / PAD_ATLAS_ROWS as f32;
                        let mut pad = sprite(
                            "openitg/selection-platform.png",
                            layout.position(x, pad_y + 9.0 * self.zoom),
                            pad_size,
                            [pad_light, pad_light, pad_light, 1.0],
                            z,
                        );
                        pad.customtexturerect([
                            u,
                            v,
                            u + 1.0 / PAD_ATLAS_COLS as f32,
                            v + 1.0 / PAD_ATLAS_ROWS as f32,
                        ]);
                        actors.push(pad.build(0));
                    }
                }
                let (english, key, size) = [
                    ("1 Player", "openitg/selection-single.png", [430.0, 164.0]),
                    ("2 Players", "openitg/selection-versus.png", [488.0, 158.0]),
                    ("Double", "openitg/selection-double.png", [338.0, 154.0]),
                ][self.index];
                label.eq_ignore_ascii_case(english).then_some((
                    key,
                    size,
                    self.y - 50.0 * self.zoom,
                ))
            }
            PlaySelectionKind::Mode => match self.index {
                0 => {
                    let x = self.x - 150.0 * self.zoom;
                    let y = self.y + 30.0 * self.zoom;
                    for key in [
                        "openitg/selection-left-foot.png",
                        "openitg/selection-right-foot.png",
                    ] {
                        actors.push(image(key, x, y, [128.0, 150.0], 0).build(0));
                    }
                    label.eq_ignore_ascii_case("regular").then_some((
                        "openitg/selection-dance.png",
                        [248.0, 64.0],
                        self.y,
                    ))
                }
                1 => {
                    actors.push(
                        image(
                            "openitg/selection-flag.png",
                            self.x - 150.0 * self.zoom,
                            self.y + 30.0 * self.zoom,
                            [154.0; 2],
                            0,
                        )
                        .build(0),
                    );
                    label.eq_ignore_ascii_case("marathon").then_some((
                        "openitg/selection-marathon.png",
                        [388.0, 64.0],
                        self.y,
                    ))
                }
                _ => None,
            },
        };
        if let Some((key, size, y)) = art {
            actors.push(image(key, self.x, y, size, 1).build(0));
        } else {
            let y = if matches!(view.kind, PlaySelectionKind::Style { .. }) {
                self.y - 50.0 * self.zoom
            } else {
                self.y
            };
            let mut text = layout.text([self.x, y], 0.5, label.into(), 1.55 * self.zoom, tint);
            text.font(TITLE_FONT);
            text.maxwidth(layout.size([408.0 * self.zoom, 0.0])[0]);
            text.z(z + 1);
            actors.push(italic(text.build(0), -0.16));
        }
    }
}

fn push_pump_pad(
    actors: &mut Vec<Actor>,
    layout: Layout,
    [x, y]: [f32; 2],
    zoom: f32,
    tint: [f32; 4],
    z: i16,
) {
    let pitch = 13.0 * zoom;
    actors.push(layout.panel(
        x - 1.6 * pitch,
        y - 1.6 * pitch,
        [3.2 * pitch; 2],
        [0.28 * tint[0], 0.22 * tint[1], 0.20 * tint[2], 1.0],
        z,
    ));
    for tile in [0, 2, 4, 6, 8] {
        let dx = tile % 3;
        let dy = tile / 3;
        actors.push(layout.panel(
            x + (dx as f32 - 1.42) * pitch,
            y + (dy as f32 - 1.42) * pitch,
            [0.84 * pitch; 2],
            [tint[0], 0.72 * tint[1], 0.25 * tint[2], 1.0],
            z + 1,
        ));
    }
}

fn push_players(actors: &mut Vec<Actor>, layout: Layout, view: &PlaySelectionView<'_>) {
    let mut help = layout.text(
        [320.0, 445.0],
        0.5,
        "Use Left or Right to choose, then press Start".into(),
        0.68,
        WHITE,
    );
    help.font(MENU_NORMAL_FONT);
    help.z(30);
    actors.push(help.build(0));
    for ((x, align), joined) in [(36.0, 0.0), (604.0, 1.0)]
        .into_iter()
        .zip(view.player_names)
    {
        let status = match joined {
            Some(display_name) => TextContent::inline_str(display_name)
                .unwrap_or_else(|| TextContent::Owned(display_name.to_owned())),
            None => view.join_prompt.into(),
        };
        let mut text = layout.text([x, 465.0], align, status, 0.63, WHITE);
        text.font(TITLE_FONT);
        text.maxwidth(layout.size([260.0, 0.0])[0]);
        text.z(30);
        actors.push(text.build(0));
    }
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
    fn style_pads_wag_only_while_their_choice_has_focus() {
        use deadlib_present::actors::SpriteSource;

        let mut carousel = PlaySelectionCarousel::default();
        let labels = ["1 Player", "2 Players", "Double"].map(Arc::<str>::from);
        let title = Arc::from("PICK A STYLE");
        let prompt = Arc::from("Press Start");
        let choices = labels.each_ref().map(|label| PlaySelectionChoice {
            label,
            description: None,
            enabled: true,
        });
        with_current_metrics(Metrics::centered(854.0, 480.0), || {
            let mut draw = |selected, time| {
                let mut actors = Vec::new();
                carousel.push_actors(
                    &mut actors,
                    PlaySelectionView {
                        kind: PlaySelectionKind::Style { pump: false },
                        title: &title,
                        choices: &choices,
                        selected,
                        player_names: [None; 2],
                        join_prompt: &prompt,
                        exit_progress: 0.0,
                    },
                    time,
                );
                actors
                    .into_iter()
                    .filter_map(|actor| match actor {
                        Actor::Sprite {
                            source: SpriteSource::TextureStatic("openitg/selection-platform.png"),
                            uv_rect,
                            ..
                        } => uv_rect,
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            let poses = draw(0, 0.0);
            assert_eq!(poses.len(), 5);
            let neutral = poses[0];
            assert!(poses.iter().all(|pose| *pose == neutral));
            assert_eq!(
                draw(0, 0.25)
                    .iter()
                    .filter(|pose| **pose != neutral)
                    .count(),
                1
            );
            assert!(draw(1, 0.25).iter().all(|pose| *pose == neutral));
            assert_eq!(
                draw(1, 0.50)
                    .iter()
                    .filter(|pose| **pose != neutral)
                    .count(),
                2
            );
        });
    }

    #[test]
    fn mode_draws_only_available_choices_with_a_centered_carousel_and_join_prompt() {
        let mut carousel = PlaySelectionCarousel::default();
        let labels = ["Regular", "Marathon", "Premium Free"].map(Arc::<str>::from);
        let description = Arc::<str>::from("Play a regular session.");
        let title = Arc::<str>::from("SELECT MODE");
        let join_prompt = Arc::<str>::from("Press Start");
        let choices = labels.each_ref().map(|label| PlaySelectionChoice {
            label,
            description: Some(&description),
            enabled: true,
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
            assert!(actors.iter().any(|actor| matches!(actor,
                Actor::Sprite { source: deadlib_present::actors::SpriteSource::TextureStatic("openitg/selection-dance.png"), offset, .. }
                    if offset[0] == width / 2.0
            )));
            assert_eq!(text("Premium Free").is_some(), count == 3);
            assert!(text("Player One").unwrap()[0] < width / 2.0);
            assert!(text("Press Start").unwrap()[0] > width / 2.0);
        }
    }
}
