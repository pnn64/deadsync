use std::sync::Arc;

use deadlib_assets::present_dsl::cover_sprite;
use deadlib_present::actors::{Actor, SpriteSource, TextAlign, TextContent};
use deadlib_present::dsl::SpriteBuilder;
use deadsync_theme::profile_picker::{PROFILE_PICKER_VISIBLE_ROWS, ProfilePickerScreenView};
use deadsync_theme::select_music::SelectCourseView;
use deadsync_theme::selection::{
    PlayModeMenu, ProfilePickerRenderer, SelectionPresentation, WheelMediaFrame,
};
use deadsync_theme::views::PlaySelectionView;

use crate::play_selection::{MODE_MENU, push_background, push_header};
use crate::presentation::{Layout, push_footer};
use crate::resources::{NORMAL_FONT, TITLE_FONT, WHEEL_FONT};
use crate::select_music::{StepManiaMusicWheel, difficulty_color, push_wheel};

impl SelectionPresentation<Actor> for StepManiaMusicWheel {
    fn play_mode_menu(&self) -> Option<&'static PlayModeMenu> {
        Some(&MODE_MENU)
    }

    fn enter_play_selection(&mut self) {
        self.play_entered = None;
        self.play_selected = None;
    }

    fn push_play_selection(
        &mut self,
        actors: &mut Vec<Actor>,
        view: PlaySelectionView<'_>,
        now: f64,
    ) {
        self.push_play(actors, view, now);
    }

    fn profile_picker(&self) -> ProfilePickerRenderer<Actor> {
        push_profiles
    }

    fn push_profile_load(&self, actors: &mut Vec<Actor>, label: &Arc<str>, elapsed: f32) {
        push_background(actors);
        let mut text = Layout::current().text(NORMAL_FONT, label.into(), [320.0, 240.0], 1.0);
        text.alpha((elapsed / 0.3).clamp(0.0, 1.0));
        actors.push(text.build(0));
    }

    fn push_course(
        &self,
        actors: &mut Vec<Actor>,
        view: SelectCourseView<'_>,
        media: WheelMediaFrame<'_>,
    ) {
        let layout = Layout::current();
        let elapsed = self.entered.map_or(0.0, |time| {
            media.now.saturating_duration_since(time).as_secs_f32()
        });
        let alpha =
            (elapsed / 0.5).clamp(0.0, 1.0) * (1.0 - media.exit_elapsed / 0.5).clamp(0.0, 1.0);
        let shift = if media.exit_elapsed > 0.0 {
            -640.0
                * deadlib_present::anim::bouncebegin_p((media.exit_elapsed / 0.5).clamp(0.0, 1.0))
        } else {
            -640.0 * (1.0 - deadlib_present::anim::bounceend_p((elapsed / 0.5).clamp(0.0, 1.0)))
        };
        push_background(actors);
        push_header(
            actors,
            "stepmania-395/course-header",
            elapsed,
            (media.exit_elapsed / 0.5).clamp(0.0, 1.0),
        );
        push_wheel(
            actors,
            layout,
            &view.rows,
            view.wheel_offset,
            elapsed,
            media.exit_elapsed,
            true,
        );
        let mut explanation = layout.sprite(
            "stepmania-395/course-explanation",
            [132.0 + shift, 68.0],
            [256.0, 48.0],
            5,
        );
        explanation.alpha(alpha);
        actors.push(explanation.build(0));
        let [x, y] = layout.position([150.0 + shift, 172.0]);
        let banner = view
            .banner_texture
            .cloned()
            .unwrap_or_else(|| Arc::from("stepmania-395/music-fallback"));
        actors.push(cover_sprite(banner, x, y, 286.0, 92.0, layout.scale, 3));
        for outgoing in media.outgoing.iter().flatten().rev() {
            let mut actor = cover_sprite(
                Arc::clone(outgoing.texture),
                x,
                y,
                286.0,
                92.0,
                layout.scale,
                3,
            );
            actor.mul_alpha(1.0 - (outgoing.elapsed_seconds / 0.25).clamp(0.0, 1.0));
            actors.push(actor);
        }
        actors.push(
            layout
                .sprite(
                    "stepmania-395/course-banner-frame",
                    [160.0 + shift, 158.0],
                    [322.0, 138.0],
                    4,
                )
                .build(0),
        );
        if let Some(item) = view.item {
            for (value, at) in [
                (item.songs, [58.0 + shift, 114.0]),
                (item.length, [192.0 + shift, 112.0]),
            ] {
                let mut label = layout.text(NORMAL_FONT, content(value), at, 0.8);
                label.z(6);
                actors.push(label.build(0));
            }
            // The original course list has five visible rows, centered at y=336.
            for (index, track) in item.tracks.iter().take(5).enumerate() {
                let Some(track) = track else { continue };
                let y = 248.0 + index as f32 * 44.0;
                let mut bar =
                    layout.sprite("stepmania-395/course-track", [160.0, y], [270.0, 44.0], 2);
                bar.alpha(alpha);
                actors.push(bar.build(0));
                let mut number = layout.text(
                    WHEEL_FONT,
                    content(&track.number.to_string()),
                    [42.0, y],
                    1.0,
                );
                number.alpha(alpha);
                actors.push(number.build(0));
                let mut title = layout.text(WHEEL_FONT, track.title.into(), [70.0, y - 8.0], 1.0);
                title.align(0.0, 0.5);
                title.horizalign(TextAlign::Left);
                title.maxwidth(180.0 * layout.scale);
                title.alpha(alpha);
                actors.push(title.build(0));
                let mut color = difficulty_color(track.difficulty);
                color[3] = alpha;
                let mut foot = layout.sprite(
                    "stepmania-395/music-feet",
                    [258.0, y - 8.0],
                    [10.0, 18.0],
                    4,
                );
                foot.customtexturerect([0.0, 0.0, 0.5, 1.0]);
                foot.diffuse(color);
                actors.push(foot.build(0));
                let mut meter = layout.text(NORMAL_FONT, track.meter.into(), [280.0, y - 8.0], 0.7);
                meter.diffuse(color);
                actors.push(meter.build(0));
            }
        }
        push_footer(
            actors,
            "&LEFT; or &RIGHT; change course     Press &START; to select",
            alpha,
        );
        self.push_options_prompt(actors, view.prompt, media.now);
    }
}

fn content(value: &str) -> TextContent {
    TextContent::inline_str(value).unwrap_or_else(|| TextContent::Owned(value.to_owned()))
}

/// The reference has no local profile catalog. Present the host's picker using
/// its options page and cursors, including the same late-join overlay contract.
fn push_profiles(actors: &mut Vec<Actor>, view: ProfilePickerScreenView<'_>, overlay: bool) {
    let layout = Layout::current();
    let start = actors.len();
    if overlay {
        actors.push(layout.solid([320.0, 240.0], [640.0, 480.0], [0.0, 0.0, 0.0, 0.8], 1450));
    }
    let z = if overlay { 1451 } else { -100 };
    actors.push(
        layout
            .sprite(
                "stepmania-395/options-page",
                [320.0, 240.0],
                [640.0, 480.0],
                z,
            )
            .build(0),
    );
    actors.push(
        layout
            .sprite(
                "stepmania-395/profile-header",
                [320.0, 22.0],
                [640.0, 44.0],
                z + 1,
            )
            .build(0),
    );
    let mut title = layout.text(TITLE_FONT, view.title.into(), [270.0, 20.0], 0.8);
    title.diffuse([0.0, 0.0, 0.0, 1.0]);
    title.z(z + 2);
    actors.push(title.build(0));
    for (side, panel) in view.panels.into_iter().enumerate() {
        let x = if side == 0 { 160.0 } else { 480.0 };
        let color = if panel.invalid_choice {
            [1.0, 0.2, 0.2, 1.0]
        } else if side == 0 {
            [0.4, 1.0, 0.8, 1.0]
        } else {
            [1.0, 0.5, 0.2, 1.0]
        };
        let mut heading = layout.text(
            NORMAL_FONT,
            if side == 0 { "PLAYER 1" } else { "PLAYER 2" }.into(),
            [x, 102.0],
            1.0,
        );
        heading.diffuse(color);
        heading.z(z + 2);
        actors.push(heading.build(0));
        if panel.joined {
            for (index, name) in panel.names.iter().enumerate() {
                let Some(name) = name else { continue };
                let y = 166.0 + index as f32 * 34.0;
                if index == PROFILE_PICKER_VISIBLE_ROWS / 2 {
                    let mut cursor =
                        layout.sprite("stepmania-395/options-cursor", [x, y], [260.0, 30.0], z + 2);
                    cursor.customtexturerect([
                        0.0,
                        side as f32 * 0.5,
                        1.0,
                        (side + 1) as f32 * 0.5,
                    ]);
                    actors.push(cursor.build(0));
                }
                let mut label = layout.text(NORMAL_FONT, (*name).into(), [x, y], 0.75);
                label.maxwidth(245.0 * layout.scale);
                label.z(z + 3);
                label.diffuse(if index == PROFILE_PICKER_VISIBLE_ROWS / 2 {
                    [0.0, 0.0, 0.0, 1.0]
                } else {
                    [0.4, 0.4, 0.4, 1.0]
                });
                actors.push(label.build(0));
            }
            if let Some(avatar) = panel.avatar {
                let mut image =
                    SpriteBuilder::with_source(SpriteSource::Texture(Arc::clone(avatar)));
                let [px, py] = layout.position([x - 80.0, 360.0]);
                image.xy(px, py);
                image.size(48.0 * layout.scale, 48.0 * layout.scale);
                image.z(z + 2);
                actors.push(image.build(0));
            }
            if let Some(count) = panel.songs_played {
                let mut text = layout.text(NORMAL_FONT, count.into(), [x, 360.0], 0.5);
                text.maxwidth(170.0 * layout.scale);
                text.z(z + 2);
                actors.push(text.build(0));
            }
        }
        if !panel.joined || panel.ready {
            let mut status = layout.text(
                NORMAL_FONT,
                if panel.ready {
                    view.waiting_text
                } else {
                    view.join_text
                }
                .into(),
                [x, 414.0],
                0.65,
            );
            status.diffuse(color);
            status.z(z + 3);
            actors.push(status.build(0));
        }
    }
    for actor in &mut actors[start..] {
        actor.mul_alpha(view.alpha);
    }
}
