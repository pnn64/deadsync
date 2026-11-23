use crate::scene::{AMBER, MINT, MUTED, PAPER, Scene};
use deadlib_present::actors::Actor;
use deadlib_present::cache::{SharedStrCache, cached_shared_str, shared_str_cache_with_capacity};
use deadsync_theme::profile_picker::{PROFILE_PICKER_VISIBLE_ROWS, ProfilePickerScreenView};
use deadsync_theme::select_music::{
    SELECT_MUSIC_WHEEL_ROWS, SelectCourseView, SelectMusicPrompt, SelectMusicView,
    SelectMusicWheelRow,
};
use deadsync_theme::selection::{ProfilePickerRenderer, SelectionPresentation, WheelMediaFrame};
use deadsync_theme::views::PlaySelectionView;
use std::{
    cell::RefCell,
    sync::Arc,
    time::{Duration, Instant},
};

const TEXT_CACHE_LIMIT: usize = 4096;

thread_local! {
    static LABELS: RefCell<SharedStrCache> =
        RefCell::new(shared_str_cache_with_capacity(TEXT_CACHE_LIMIT));
}

fn label(source: &str) -> Arc<str> {
    cached_shared_str(&LABELS, source, TEXT_CACHE_LIMIT)
}

pub struct CircuitSelection;

impl SelectionPresentation<Actor> for CircuitSelection {
    fn show_random_entries(&self) -> bool {
        false
    }

    fn banner_crossfade(&self) -> Option<Duration> {
        None
    }

    fn player_options_exit_seconds(&self) -> Option<f32> {
        None
    }

    fn enter_wheel(&mut self, _: Instant) {}

    fn confirm_wheel(&mut self, _: Instant) {}

    fn cancel_wheel_confirmation(&mut self) {}

    fn enter_play_selection(&mut self) {}

    fn push_music(
        &self,
        actors: &mut Vec<Actor>,
        view: SelectMusicView<'_>,
        _: WheelMediaFrame<'_>,
    ) {
        let scene = wheel_scene(actors, "SELECT TRACK", view.rows, view.wheel_offset);
        draw_banner(scene, actors, view.banner_texture, view.selection_beat);
        if let Some(song) = view.song {
            scene.text(
                actors,
                [36.0, 256.0],
                [1.05, 284.0],
                label(song.title),
                PAPER,
            );
            scene.text(
                actors,
                [36.0, 289.0],
                [0.62, 284.0],
                label(song.artist),
                MUTED,
            );
            scene.text(
                actors,
                [36.0, 316.0],
                [0.62, 284.0],
                label(song.display_bpm),
                MINT,
            );
        }
        for (side, player) in view.players.iter().enumerate() {
            let x = 36.0 + side as f32 * 146.0;
            let accent = if side == 0 { MINT } else { AMBER };
            scene.rect(actors, [x, 347.0], [136.0, 62.0], accent, 0.09, 0);
            if player.joined {
                scene.text(
                    actors,
                    [x + 8.0, 364.0],
                    [0.52, 120.0],
                    label(player.name),
                    PAPER,
                );
                if let Some(chart) = player.chart {
                    scene.text(
                        actors,
                        [x + 8.0, 387.0],
                        [0.66, 121.0],
                        label(&chart.difficulty),
                        accent,
                    );
                }
            } else {
                scene.text(
                    actors,
                    [x + 8.0, 379.0],
                    [0.48, 121.0],
                    "PRESS START",
                    MUTED,
                );
            }
        }
        prompt(scene, actors, view.prompt);
    }

    fn push_course(
        &self,
        actors: &mut Vec<Actor>,
        view: SelectCourseView<'_>,
        _: WheelMediaFrame<'_>,
    ) {
        let scene = wheel_scene(actors, "SELECT COURSE", view.rows, view.wheel_offset);
        draw_banner(scene, actors, view.banner_texture, view.selection_beat);
        if let Some(item) = view.item {
            scene.text(
                actors,
                [36.0, 252.0],
                [1.1, 284.0],
                label(item.title),
                PAPER,
            );
            if let Some(rating) = item.rating {
                scene.text(actors, [36.0, 287.0], [0.67, 282.0], label(rating), MINT);
            }
            for (index, track) in item.tracks.iter().flatten().take(4).enumerate() {
                scene.text(
                    actors,
                    [36.0, 318.0 + index as f32 * 23.0],
                    [0.6, 280.0],
                    track.title,
                    MUTED,
                );
            }
        }
        prompt(scene, actors, view.prompt);
    }

    fn profile_picker(&self) -> ProfilePickerRenderer<Actor> {
        profile_picker
    }

    fn push_play_selection(
        &mut self,
        actors: &mut Vec<Actor>,
        view: PlaySelectionView<'_>,
        _: f64,
    ) {
        let scene = Scene::new(1.0 - view.exit_progress.clamp(0.0, 1.0));
        scene.cover(actors);
        scene.text(actors, [48.0, 68.0], [1.25, 544.0], view.title, PAPER);
        let Some(choice) = view.choices.get(view.selected) else {
            return;
        };
        // Center on the controller's selection so longer choice lists fit the canvas.
        let start = view
            .selected
            .saturating_sub(2)
            .min(view.choices.len().saturating_sub(5));
        for (row, item) in view.choices.iter().skip(start).take(5).enumerate() {
            let y = 121.0 + row as f32 * 48.0;
            let selected = row + start == view.selected;
            let ink = if selected { MINT } else { MUTED };
            scene.rect(
                actors,
                [48.0, y],
                [544.0, 38.0],
                ink,
                if selected { 0.17 } else { 0.065 },
                0,
            );
            scene.text(actors, [62.0, y + 19.0], [0.89, 515.0], item.label, ink);
        }
        if let Some(description) = choice.description {
            scene.text(actors, [48.0, 386.0], [0.64, 544.0], description, PAPER);
        }
        scene.text(
            actors,
            [48.0, 430.0],
            [0.54, 544.0],
            view.join_prompt,
            MUTED,
        );
    }

    fn push_profile_load(&self, actors: &mut Vec<Actor>, title: &Arc<str>, elapsed: f32) {
        let scene = Scene::new(1.0);
        scene.cover(actors);
        scene.motif(actors, [45.0, 235.0], elapsed);
        scene.text(actors, [45.0, 285.0], [1.3, 548.0], title, PAPER);
    }
}

fn wheel_scene(
    actors: &mut Vec<Actor>,
    title: &'static str,
    rows: [Option<SelectMusicWheelRow<'_>>; SELECT_MUSIC_WHEEL_ROWS],
    offset: f32,
) -> Scene {
    let scene = Scene::new(1.0);
    scene.cover(actors);
    scene.text(actors, [36.0, 57.0], [0.84, 560.0], title, MINT);
    let center = SELECT_MUSIC_WHEEL_ROWS / 2;
    for (index, row) in rows.iter().enumerate() {
        let Some(row) = row else {
            continue;
        };
        let y = 252.0 + (index as f32 - center as f32 + offset) * 40.0;
        if !(102.0..=406.0).contains(&y) {
            continue;
        }
        let active = index == center;
        let ink = if active {
            MINT
        } else if row.song_count.is_some() {
            AMBER
        } else {
            MUTED
        };
        scene.rect(
            actors,
            [341.0, y - 16.0],
            [264.0, 32.0],
            ink,
            if active { 0.19 } else { 0.065 },
            0,
        );
        scene.text(actors, [354.0, y], [0.71, 238.0], label(row.title), ink);
    }
    scene
}

fn draw_banner(scene: Scene, actors: &mut Vec<Actor>, banner: Option<&Arc<str>>, beat: f32) {
    scene.rect(actors, [36.0, 92.0], [284.0, 133.0], MUTED, 0.11, 0);
    if let Some(banner) = banner {
        scene.banner(actors, [178.0, 158.5], [284.0, 133.0], banner);
    } else {
        scene.motif(actors, [89.0, 193.0], beat * 0.5);
    }
}

fn prompt(scene: Scene, actors: &mut Vec<Actor>, prompt: SelectMusicPrompt) {
    let (text, ink) = match prompt {
        SelectMusicPrompt::OfferOptions => ("START AGAIN FOR PLAYER OPTIONS", AMBER),
        SelectMusicPrompt::EnteringOptions => ("ENTERING PLAYER OPTIONS", AMBER),
        SelectMusicPrompt::Hidden => ("START TO CONFIRM", MUTED),
    };
    scene.text(actors, [36.0, 435.0], [0.55, 568.0], text, ink);
}

fn profile_picker(actors: &mut Vec<Actor>, view: ProfilePickerScreenView<'_>, overlay: bool) {
    let scene = Scene::new(view.alpha);
    if !overlay {
        scene.cover(actors);
    }
    scene.text(actors, [48.0, 64.0], [1.12, 544.0], view.title, PAPER);
    for (side, panel) in view.panels.iter().enumerate() {
        let x = 48.0 + side as f32 * 280.0;
        let accent = if side == 0 { MINT } else { AMBER };
        scene.rect(
            actors,
            [x, 118.0],
            [264.0, 270.0],
            accent,
            if overlay { 0.62 } else { 0.10 },
            0,
        );
        scene.text(
            actors,
            [x + 14.0, 143.0],
            [0.64, 236.0],
            if side == 0 { "PLAYER 1" } else { "PLAYER 2" },
            accent,
        );
        if !panel.joined {
            scene.text(
                actors,
                [x + 14.0, 256.0],
                [0.69, 236.0],
                view.join_text,
                MUTED,
            );
            continue;
        }
        for (row, name) in panel.names.iter().enumerate() {
            if let Some(name) = name {
                let y = 187.0 + row as f32 * 33.0;
                let focused = row == PROFILE_PICKER_VISIBLE_ROWS / 2;
                if focused {
                    scene.rect(actors, [x + 9.0, y - 15.0], [246.0, 30.0], accent, 0.18, 1);
                }
                scene.text(
                    actors,
                    [x + 17.0, y],
                    [0.78, 227.0],
                    *name,
                    if focused { PAPER } else { MUTED },
                );
            }
        }
        if panel.ready {
            scene.text(actors, [x + 14.0, 365.0], [0.6, 235.0], "READY", accent);
        }
    }
    scene.text(
        actors,
        [48.0, 429.0],
        [0.56, 544.0],
        view.waiting_text,
        MUTED,
    );
}
