use deadlib_present::{actors::Actor, anim};
use deadsync_theme::selection::{PlayModeMenu, PlayModeMenuChoice, PlaySelectionMode};
use deadsync_theme::views::{PlaySelectionKind, PlaySelectionView};

use crate::{
    presentation::{Layout, push_footer, push_style_icon},
    select_music::StepManiaMusicWheel,
};

pub(crate) static MODE_MENU: PlayModeMenu = PlayModeMenu {
    choices: &[
        mode("BEGINNER", Some(PlaySelectionMode::Regular), Some(0)),
        mode("LIGHT", Some(PlaySelectionMode::Regular), Some(1)),
        mode("STANDARD", Some(PlaySelectionMode::Regular), Some(2)),
        mode("HEAVY", Some(PlaySelectionMode::Regular), Some(3)),
        mode("NONSTOP", Some(PlaySelectionMode::Course), None),
        mode("CHALLENGE", None, None),
        mode("ENDLESS", None, None),
        mode("MAGIC DANCE", None, None),
    ],
};

const fn mode(
    label: &'static str,
    mode: Option<PlaySelectionMode>,
    difficulty_index: Option<usize>,
) -> PlayModeMenuChoice {
    PlayModeMenuChoice {
        label,
        mode,
        difficulty_index,
    }
}

const STYLE_ICONS: [&str; 5] = [
    "stepmania-395/style-icon-1",
    "stepmania-395/style-icon-2",
    "stepmania-395/style-icon-3",
    "stepmania-395/style-icon-4",
    "stepmania-395/style-icon-5",
];
const STYLE_INFO: [&str; 5] = [
    "stepmania-395/style-info-1",
    "stepmania-395/style-info-2",
    "stepmania-395/style-info-3",
    "stepmania-395/style-info-4",
    "stepmania-395/style-info-5",
];
const STYLE_PICTURES: [(&str, [f32; 2]); 5] = [
    ("stepmania-395/style-picture-1", [216.0, 280.0]),
    ("stepmania-395/style-picture-2", [334.0, 346.0]),
    ("stepmania-395/style-picture-3", [334.0, 346.0]),
    ("stepmania-395/style-picture-4", [336.0, 352.0]),
    ("stepmania-395/style-picture-5", [220.0, 334.0]),
];
const MODE_INFO: [&str; 8] = [
    "stepmania-395/mode-info-1",
    "stepmania-395/mode-info-2",
    "stepmania-395/mode-info-3",
    "stepmania-395/mode-info-4",
    "stepmania-395/mode-info-5",
    "stepmania-395/mode-info-6",
    "stepmania-395/mode-info-7",
    "stepmania-395/mode-info-8",
];
const MODE_PICTURES: [&str; 8] = [
    "stepmania-395/mode-picture-1",
    "stepmania-395/mode-picture-2",
    "stepmania-395/mode-picture-3",
    "stepmania-395/mode-picture-4",
    "stepmania-395/mode-picture-5",
    "stepmania-395/mode-picture-6",
    "stepmania-395/mode-picture-7",
    "stepmania-395/mode-picture-8",
];

pub(crate) fn push_background(actors: &mut Vec<Actor>) {
    actors.push(
        Layout::current()
            .sprite(
                "stepmania-395/music-background",
                [320.0, 240.0],
                [640.0, 480.0],
                -100,
            )
            .build(0),
    );
}

pub(crate) fn push_header(actors: &mut Vec<Actor>, key: &'static str, elapsed: f32, exit: f32) {
    let shift = header_shift(elapsed, exit);
    actors.push(
        Layout::current()
            .sprite(key, [320.0 + shift, 22.0], [640.0, 44.0], 95)
            .build(0),
    );
}

pub(crate) fn header_shift(elapsed: f32, exit: f32) -> f32 {
    640.0 * (1.0 - anim::spring_p((elapsed / 0.5).clamp(0.0, 1.0)) + anim::bouncebegin_p(exit))
}

impl StepManiaMusicWheel {
    pub(crate) fn push_play(
        &mut self,
        actors: &mut Vec<Actor>,
        view: PlaySelectionView<'_>,
        now: f64,
    ) {
        let entered = *self.play_entered.get_or_insert(now);
        if self.play_selected != Some(view.selected) {
            self.play_selected = Some(view.selected);
            self.focus_started = now;
        }
        let elapsed = (now - entered).max(0.0) as f32;
        let focus = (now - self.focus_started).max(0.0) as f32;
        push_background(actors);
        match view.kind {
            PlaySelectionKind::Style { .. } => push_style(actors, &view, elapsed, focus),
            PlaySelectionKind::Mode => push_mode(actors, &view, elapsed),
        }
        push_footer(
            actors,
            "&LEFT; or &RIGHT; to change selection     Press &START; to select",
            (elapsed / 0.5).clamp(0.0, 1.0) * (1.0 - view.exit_progress),
        );
    }
}

fn push_style(actors: &mut Vec<Actor>, view: &PlaySelectionView<'_>, elapsed: f32, focus: f32) {
    let layout = Layout::current();
    let exit = view.exit_progress.clamp(0.0, 1.0);
    let selected = [0, 1, 3][view.selected.min(2)];
    push_header(actors, "stepmania-395/style-header", elapsed, exit);
    for (index, key) in STYLE_ICONS.into_iter().enumerate() {
        let t = ((elapsed - (4 - index) as f32 * 0.05) / 0.3).clamp(0.0, 1.0);
        let mut icon = layout.sprite(
            key,
            [60.0 + index as f32 * 80.0 - 640.0 * (1.0 - t).powi(2), 84.0],
            [64.0; 2],
            2,
        );
        icon.zoomy(1.0 - anim::bouncebegin_p(exit));
        if [2, 4].contains(&index) {
            icon.diffuse([0.2, 0.2, 0.2, 1.0]);
        }
        if selected == index {
            icon.glow([
                1.0,
                1.0,
                1.0,
                0.5 - 0.3 * (focus * std::f32::consts::TAU / 0.5).cos(),
            ]);
        }
        actors.push(icon.build(0));
    }
    let shift = 384.0
        * (1.0 - anim::bounceend_p((elapsed / 0.5).clamp(0.0, 1.0)) + anim::bouncebegin_p(exit));
    actors.push(
        layout
            .sprite(
                "stepmania-395/style-explanation",
                [480.0 + shift, 170.0],
                [282.0, 38.0],
                3,
            )
            .build(0),
    );
    let mut warning = layout.sprite(
        "stepmania-395/style-warning",
        [
            480.0,
            400.0 - 120.0 * (1.0 - (elapsed / 0.5).clamp(0.0, 1.0)).powi(2),
        ],
        [282.0, 64.0],
        3,
    );
    warning.zoomy((1.0 - (1.0 - elapsed / 0.5).clamp(0.0, 1.0).powi(2)) * (1.0 - exit.powi(2)));
    let intensity = 0.7 + 0.3 * (elapsed * std::f32::consts::TAU).cos();
    warning.diffuse([intensity, intensity, intensity, 1.0]);
    actors.push(warning.build(0));
    let mut info = layout.sprite(STYLE_INFO[selected], [480.0, 280.0], [286.0, 164.0], 4);
    info.zoomy(
        anim::bounceend_p((focus / 0.5).clamp(0.0, 1.0)) * (1.0 - anim::bouncebegin_p(exit)),
    );
    actors.push(info.build(0));
    let (key, size) = STYLE_PICTURES[selected];
    let mut picture = layout.sprite(key, [160.0, 260.0], size, 3);
    picture.alpha(if focus >= 0.2 { 1.0 - exit } else { 0.0 });
    picture.glow([
        1.0,
        1.0,
        1.0,
        (1.0 - ((focus - 0.2) / 0.2).abs()).clamp(0.0, 1.0) * (1.0 - exit),
    ]);
    actors.push(picture.build(0));
}

fn push_mode(actors: &mut Vec<Actor>, view: &PlaySelectionView<'_>, elapsed: f32) {
    let layout = Layout::current();
    let exit = view.exit_progress.clamp(0.0, 1.0);
    let page = view.selected / 4;
    push_header(actors, "stepmania-395/mode-header", elapsed, exit);
    push_style_icon(actors, view.session_style, elapsed, exit);
    let mut explanation = layout.sprite(
        "stepmania-395/mode-explanation",
        [if page == 0 { 170.0 } else { 460.0 }, 70.0],
        [256.0, 38.0],
        3,
    );
    explanation.customtexturerect([0.0, page as f32 * 0.5, 1.0, (page + 1) as f32 * 0.5]);
    explanation.alpha((elapsed / 0.5).clamp(0.0, 1.0) * (1.0 - exit));
    actors.push(explanation.build(0));
    let more_key = if page == 0 {
        "stepmania-395/mode-more-1"
    } else {
        "stepmania-395/mode-more-2"
    };
    let mut more = layout.sprite(
        more_key,
        [if page == 0 { 580.0 } else { 60.0 }, 90.0],
        [64.0; 2],
        3,
    );
    let frame = ((elapsed / 0.1) as usize % 4) as f32;
    more.customtexturerect([0.0, frame * 0.25, 1.0, (frame + 1.0) * 0.25]);
    more.alpha((elapsed / 0.5).clamp(0.0, 1.0) * (1.0 - exit));
    actors.push(more.build(0));
    for column in 0..4 {
        let index = page * 4 + column;
        let Some(choice) = view.choices.get(index) else {
            continue;
        };
        let x = 98.0 + column as f32 * 148.0;
        let y = if page == 0 && column.is_multiple_of(2) {
            180.0
        } else {
            220.0
        };
        let delay = column as f32 * 0.2;
        let enter = ((elapsed - delay) / 0.4).clamp(0.0, 1.0);
        let shift = if page == 0 {
            -768.0 * (1.0 - enter).powi(2)
        } else {
            0.0
        };
        let leave = ((exit * 0.9 - 0.3 - delay) / 0.4).clamp(0.0, 1.0);
        let mut info = layout.sprite(
            MODE_INFO[index],
            [x + shift - leave * leave * 768.0, y - 40.0],
            [192.0, 80.0],
            3,
        );
        let mut picture = layout.sprite(MODE_PICTURES[index], [x, y + 108.0], [192.0, 216.0], 2);
        let zoom = anim::bounceend_p(((elapsed - 0.4 - delay) / 0.3).clamp(0.0, 1.0))
            * (1.0 - anim::bouncebegin_p(((exit * 0.9 - delay) / 0.3).clamp(0.0, 1.0)));
        picture.align(0.5, 0.0);
        // Keep the authored top edge fixed as the character card unfolds.
        picture.xy(layout.position([x, y])[0], layout.position([x, y])[1]);
        picture.zoomy(zoom);
        if !choice.enabled {
            info.diffuse([0.2, 0.2, 0.2, 1.0]);
            picture.diffuse([0.2, 0.2, 0.2, 1.0]);
        }
        actors.push(picture.build(0));
        actors.push(info.build(0));
    }
    let column = view.selected % 4;
    let y = if page == 0 && column.is_multiple_of(2) {
        180.0
    } else {
        220.0
    };
    let progress = (elapsed / 0.3).clamp(0.0, 1.0);
    for (seat, joined) in view.player_names.iter().enumerate() {
        if joined.is_none() {
            continue;
        }
        let x = 98.0 + column as f32 * 148.0 + if seat == 0 { -40.0 } else { 40.0 };
        let size = (4.0 - 3.0 * progress) * (1.0 - ((exit * 0.9 - 0.2) / 0.3).clamp(0.0, 1.0));
        for (key, offset, alpha, z) in [
            ("stepmania-395/mode-shadow", 10.0, 1.0 - exit, 4),
            ("stepmania-395/mode-cursor", 0.0, 1.0, 5),
        ] {
            let mut cursor = layout.sprite(
                key,
                [x + offset + 10.0 * exit, y + 188.0 + offset + 10.0 * exit],
                [74.0, 70.0],
                z,
            );
            cursor.customtexturerect([seat as f32 * 0.5, 0.0, (seat + 1) as f32 * 0.5, 1.0]);
            cursor.zoom(size);
            cursor.rotationz(-20.0 * (1.0 - progress));
            cursor.alpha(progress * alpha);
            actors.push(cursor.build(0));
        }
        if exit > 0.0 {
            let mut ok =
                layout.sprite("stepmania-395/mode-ok", [x + 10.0, y + 198.0], [64.0; 2], 6);
            ok.customtexturerect([seat as f32 * 0.5, 0.0, (seat + 1) as f32 * 0.5, 1.0]);
            ok.zoom((2.0 - (exit * 0.9 / 0.2).clamp(0.0, 1.0)) * size);
            actors.push(ok.build(0));
        }
    }
}
