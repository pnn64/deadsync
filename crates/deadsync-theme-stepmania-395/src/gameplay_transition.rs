use deadlib_present::actors::Actor;
use deadsync_theme::UiCue;
use deadsync_theme::stage::StageOutroPresentation;

use crate::presentation::Layout;

const ENTRY_ROWS: [[f32; 2]; 6] = [
    [-64.0, 64.0],
    [-128.0, 128.0],
    [-192.0, 192.0],
    [-256.0, 256.0],
    [-320.0, 320.0],
    [-384.0, 384.0],
];
const EXIT_RIGHT: [[f32; 2]; 7] = [
    [-64.0, 48.0],
    [-128.0, 112.0],
    [-192.0, 176.0],
    [-256.0, 240.0],
    [-192.0, 304.0],
    [-128.0, 368.0],
    [-64.0, 432.0],
];
const EXIT_LEFT: [[f32; 2]; 8] = [
    [64.0, 16.0],
    [128.0, 80.0],
    [192.0, 144.0],
    [256.0, 208.0],
    [256.0, 272.0],
    [192.0, 336.0],
    [128.0, 400.0],
    [64.0, 464.0],
];

#[derive(Clone, Copy)]
enum WipeKind {
    Entry,
    Cleared,
    Failed,
}

pub(crate) fn push_entry(actors: &mut Vec<Actor>, elapsed: f32) {
    let layout = Layout::current();
    if elapsed < 1.3 {
        let alpha = 1.0 - ((elapsed - 0.5) / 0.8).clamp(0.0, 1.0);
        actors.push(layout.solid([320.0, 240.0], [640.0, 480.0], [0.0, 0.0, 0.0, alpha], 1000));
    }
    if elapsed < 2.0 {
        let left = ENTRY_ROWS.map(|[x, y]| [x + 1088.0, y + 32.0]);
        push_wipe(
            actors,
            &ENTRY_ROWS,
            &left,
            elapsed / 2.0,
            1088.0,
            WipeKind::Entry,
        );
    }
    if (2.0..3.5).contains(&elapsed) {
        focusing(
            actors,
            "stepmania-395/gameplay-ready",
            [366.0, 78.0],
            elapsed - 2.0,
        );
    } else if (3.5..5.0).contains(&elapsed) {
        focusing(
            actors,
            "stepmania-395/gameplay-go",
            [456.0, 66.0],
            elapsed - 3.5,
        );
    }
}

fn focusing(actors: &mut Vec<Actor>, key: &'static str, size: [f32; 2], elapsed: f32) {
    let layout = Layout::current();
    let progress = if elapsed < 0.5 {
        elapsed * 2.0
    } else if elapsed < 1.0 {
        1.0
    } else {
        3.0 - elapsed * 2.0
    };
    for direction in [0.0, -1.0, 1.0] {
        let distance = direction * 100.0 * (1.0 - progress);
        let mut word = layout.sprite(key, [320.0 + distance, 240.0 + distance], size, 1100);
        word.alpha(progress * 0.5);
        actors.push(word.build(0));
    }
}

pub(crate) struct StageOutro {
    pub failed: bool,
}

impl StageOutroPresentation<Actor> for StageOutro {
    fn duration(&self) -> f32 {
        5.0
    }
    fn cue(&self) -> Option<UiCue> {
        self.failed.then_some(UiCue::StageFailed)
    }
    fn covers_stage(&self, elapsed: f32) -> bool {
        elapsed >= if self.failed { 1.2 } else { 2.0 }
    }
    fn stop_tail_audio_when_frozen(&self) -> bool {
        self.failed
    }
    fn push_actors(&self, actors: &mut Vec<Actor>, elapsed: f32) {
        let duration = if self.failed { 1.2 } else { 2.0 };
        let distance = if self.failed { 1216.0 } else { 960.0 };
        let mut left = EXIT_LEFT.map(|[x, y]| [x + 640.0, y]);
        if self.failed {
            for (index, row) in left.iter_mut().enumerate() {
                row[0] = 1152.0 - index as f32 * 64.0;
            }
        }
        push_wipe(
            actors,
            &EXIT_RIGHT,
            &left,
            (elapsed / duration).clamp(0.0, 1.0),
            distance,
            if self.failed {
                WipeKind::Failed
            } else {
                WipeKind::Cleared
            },
        );
        let layout = Layout::current();
        if self.failed {
            for delay in [0.5, 0.6, 0.7, 0.8] {
                let time = elapsed - delay;
                if !(0.0..3.8).contains(&time) {
                    continue;
                }
                let zoom = if time < 0.6 {
                    0.6 + 3.4 * (1.0 - time / 0.6).powi(2)
                } else if time < 1.0 {
                    0.6 + 0.4 * ((time - 0.6) / 0.4).powi(2)
                } else if time < 1.2 {
                    1.0 + 0.1 * (1.0 - (1.0 - (time - 1.0) / 0.2).powi(2))
                } else {
                    1.0 + 0.1 * (1.0 - ((time - 1.2) / 0.1).clamp(0.0, 1.0))
                };
                let alpha = 0.4
                    * (1.0 - (1.0 - time / 0.6).clamp(0.0, 1.0).powi(2))
                    * (1.0 - ((time - 3.3) / 0.5).clamp(0.0, 1.0));
                let mut word = layout.sprite(
                    "stepmania-395/gameplay-failed",
                    [320.0, 240.0],
                    [306.0, 94.0],
                    1100,
                );
                word.zoom(zoom);
                word.alpha(alpha);
                actors.push(word.build(0));
            }
        } else {
            let alpha = ((elapsed - 1.5) / 0.5).clamp(0.0, 1.0)
                * (1.0 - ((elapsed - 4.0) / 0.5).clamp(0.0, 1.0));
            let mut word = layout.sprite(
                "stepmania-395/gameplay-cleared",
                [320.0, 240.0],
                [414.0, 110.0],
                1100,
            );
            word.alpha(alpha);
            actors.push(word.build(0));
        }
    }
}

fn push_wipe(
    actors: &mut Vec<Actor>,
    right: &[[f32; 2]],
    left: &[[f32; 2]],
    progress: f32,
    distance: f32,
    kind: WipeKind,
) {
    let layout = Layout::current();
    let (right_key, left_key, opening) = match kind {
        WipeKind::Entry => (
            "stepmania-395/gameplay-in-right",
            "stepmania-395/gameplay-in-left",
            true,
        ),
        WipeKind::Cleared => (
            "stepmania-395/gameplay-out-right",
            "stepmania-395/gameplay-out-left",
            false,
        ),
        WipeKind::Failed => (
            "stepmania-395/gameplay-failed-right",
            "stepmania-395/gameplay-failed-left",
            false,
        ),
    };
    for (rows, direction, key) in [(right, 1.0, right_key), (left, -1.0, left_key)] {
        for &[x, y] in rows {
            let x = x + direction * distance * progress;
            actors.push(layout.sprite(key, [x, y], [128.0, 32.0], 1002).build(0));
            let side = if opening { direction } else { -direction };
            actors.push(layout.solid(
                [x + side * (64.0 + distance * 0.5), y],
                [distance, 32.0],
                [0.0, 0.0, 0.0, 1.0],
                1001,
            ));
        }
    }
}
