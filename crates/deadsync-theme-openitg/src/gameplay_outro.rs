use std::path::Path;

use deadlib_present::actors::Actor;
use deadlib_present::space::{screen_height, screen_width};
use deadsync_theme::UiCue;

use crate::presentation::{Layout, quad, sprite};

const DEPTH: i16 = 1200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayOutro {
    Cleared,
    Failed,
    Pandemonium,
    VertexTwo,
}

impl GameplayOutro {
    /// Resolve the two song-specific failure actors by their original pack and folder.
    pub fn from_result(failed: bool, simfile: &Path) -> Self {
        if !failed {
            return Self::Cleared;
        }
        let Some(song) = simfile.parent() else {
            return Self::Failed;
        };
        let Some(song_name) = song.file_name().and_then(|name| name.to_str()) else {
            return Self::Failed;
        };
        let Some(pack) = song
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
        else {
            return Self::Failed;
        };
        if pack.eq_ignore_ascii_case("In The Groove")
            && song_name.eq_ignore_ascii_case("Pandemonium")
        {
            Self::Pandemonium
        } else if pack.eq_ignore_ascii_case("In The Groove 2")
            && song_name.eq_ignore_ascii_case("VerTex^2")
        {
            Self::VertexTwo
        } else {
            Self::Failed
        }
    }

    pub const fn duration(self) -> f32 {
        match self {
            Self::Cleared => 0.8,
            Self::Failed | Self::Pandemonium | Self::VertexTwo => 7.5,
        }
    }

    pub const fn cue(self) -> Option<UiCue> {
        match self {
            Self::Cleared => None,
            Self::Failed | Self::Pandemonium | Self::VertexTwo => Some(UiCue::StageFailed),
        }
    }

    const fn curtain_seconds(self) -> f32 {
        match self {
            Self::Cleared => self.duration(),
            Self::Failed | Self::Pandemonium | Self::VertexTwo => 1.5,
        }
    }

    /// The renderer can omit the stage once the opaque curtain covers it.
    pub const fn covers_stage(self, elapsed: f32) -> bool {
        elapsed >= self.curtain_seconds()
    }

    pub fn push_actors(self, actors: &mut Vec<Actor>, elapsed: f32) {
        let viewport = [screen_width(), screen_height()];
        actors.push(quad(
            [0.0; 2],
            viewport,
            [
                0.0,
                0.0,
                0.0,
                (elapsed / self.curtain_seconds()).clamp(0.0, 1.0),
            ],
            DEPTH,
        ));
        if self == Self::Cleared {
            return;
        }

        let layout = Layout::centered(viewport[0], viewport[1]);
        let [center_x, center_y] = viewport.map(|axis| axis * 0.5);
        let center = [center_x, center_y];
        if self == Self::Pandemonium && (3.0..7.0).contains(&elapsed) {
            let (zoom, opacity) = if elapsed < 6.0 {
                (3.6 - 0.4 * (elapsed - 3.0), (elapsed - 3.0).min(1.0))
            } else {
                (2.4 - 0.4 * (elapsed - 6.0), 7.0 - elapsed)
            };
            let mut skull = sprite(
                "openitg/outro-pandemonium.png",
                center,
                layout.size([80.0 * zoom, 88.0 * zoom]),
                [1.0, 1.0, 1.0, opacity],
                DEPTH + 1,
            );
            skull.rotationz(30.0);
            actors.push(skull.build(0));
        }

        push_streak(
            actors,
            layout,
            center,
            ("openitg/outro-red-streak.png", 3.2, 2.0, DEPTH + 2),
            elapsed,
        );
        push_word_pair(
            actors,
            layout,
            viewport,
            elapsed,
            [
                (
                    "openitg/outro-life.png",
                    [-42.0, -32.0],
                    [298.0, 146.0],
                    [1.0, -1.0],
                ),
                (
                    "openitg/outro-depleted.png",
                    [42.0, 32.0],
                    [662.0, 152.0],
                    [1.0, 1.0],
                ),
            ],
            2.0,
            DEPTH + 3,
        );
        push_streak(
            actors,
            layout,
            center,
            ("openitg/outro-blue-streak.png", 3.6, 3.0, DEPTH + 4),
            elapsed - 2.5,
        );
        push_word_pair(
            actors,
            layout,
            viewport,
            elapsed - 2.5,
            [
                (
                    "openitg/outro-round.png",
                    [-90.0, 0.0],
                    [470.0, 146.0],
                    [1.0, -1.0],
                ),
                (
                    "openitg/outro-failed.png",
                    [100.0, 0.0],
                    [468.0, 152.0],
                    [-1.0, 1.0],
                ),
            ],
            3.0,
            DEPTH + 5,
        );

        if self == Self::VertexTwo && (4.5..6.5).contains(&elapsed) {
            let scale = ((elapsed - 4.5) / 0.2).min(1.0).powi(2);
            let leave = ((elapsed - 6.0) / 0.5).max(0.0).powi(2);
            let [dx, dy] = layout.size([290.0, 10.0]);
            let (sine, cosine) = (-30.0_f32).to_radians().sin_cos();
            let mut two = sprite(
                "openitg/outro-vertex-two.png",
                [
                    center_x + dx * cosine - dy * sine + viewport[0] * 1.3 * leave,
                    center_y + dx * sine + dy * cosine - viewport[1] * leave,
                ],
                layout.size([41.0 * scale, 45.0 * (2.0 - scale)]),
                [1.0, 1.0, 1.0, scale],
                DEPTH + 6,
            );
            two.rotationz(30.0);
            actors.push(two.build(0));
        }
    }
}

fn push_streak(
    actors: &mut Vec<Actor>,
    layout: Layout,
    center: [f32; 2],
    (key, initial_zoom, hold, depth): (&'static str, f32, f32, i16),
    elapsed: f32,
) {
    if !(0.0..hold + 2.0).contains(&elapsed) {
        return;
    }
    let enter = elapsed.min(1.0);
    let across = ((elapsed - 1.0) / hold).clamp(0.0, 1.0);
    let exit = (elapsed - 1.0 - hold).max(0.0);
    let zoom = initial_zoom - 0.4 * enter - (initial_zoom - 2.4) * across - 0.4 * exit;
    let mut streak = sprite(
        key,
        center,
        layout.size([1024.0 * zoom, 128.0 * zoom]),
        [1.0; 4],
        depth,
    );
    streak.rotationz(30.0);
    streak.fadeleft(0.3);
    streak.faderight(0.3);
    streak.cropleft(1.3 - 1.6 * enter);
    streak.cropright(1.6 * exit - 0.3);
    actors.push(streak.build(0));
}

fn push_word_pair(
    actors: &mut Vec<Actor>,
    layout: Layout,
    viewport: [f32; 2],
    elapsed: f32,
    words: [(&'static str, [f32; 2], [f32; 2], [f32; 2]); 2],
    hold: f32,
    depth: i16,
) {
    if !(0.0..hold + 1.0).contains(&elapsed) {
        return;
    }
    let enter = (1.0 - elapsed * 2.0).max(0.0).powi(2);
    let leave = ((elapsed - hold - 0.5) * 2.0).max(0.0).powi(2);
    for (index, (key, [x, y], size, [enter_x, exit_x])) in words.into_iter().enumerate() {
        let vertical_direction = if index == 0 { -1.0 } else { 1.0 };
        let [dx, dy] = layout.size([x, y]);
        let mut word = sprite(
            key,
            [
                viewport[0] * (0.5 + 1.3 * (enter_x * enter + exit_x * leave)) + dx,
                viewport[1] * (0.5 + vertical_direction * (enter - leave)) + dy,
            ],
            layout.size(size),
            [1.0; 4],
            depth,
        );
        word.rotationz(30.0);
        actors.push(word.build(0));
    }
}
