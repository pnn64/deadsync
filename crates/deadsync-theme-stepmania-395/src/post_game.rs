use deadlib_assets::AssetManager;
use deadlib_present::actors::{Actor, TextContent};
use deadsync_theme::post_game::{
    NameEntryPage, SessionEndPolicy, SessionEndPresentation, SessionSummaryPage, TimedEntryPolicy,
};

use crate::presentation::Layout;
use crate::resources::{ENTRY_FONT, TIMER_FONT, WHEEL_FONT};

#[derive(Default)]
pub struct StepManiaSessionEnd;

impl SessionEndPresentation<Actor> for StepManiaSessionEnd {
    fn policy(&self) -> SessionEndPolicy {
        SessionEndPolicy {
            summary_history: 3,
            summary_seconds: 30.0,
            summary_input_seconds: 1.3,
            game_over_seconds: 5.3,
            name_entry: TimedEntryPolicy {
                alphabet: b"    ABCDEFGHIJKLMNOPQRSTUVWXYZ",
                characters: 4,
                characters_per_second: 2.5,
                timeout_seconds: 24.0,
            },
        }
    }

    fn push_summary(
        &self,
        actors: &mut Vec<Actor>,
        assets: &AssetManager,
        page: SessionSummaryPage<'_>,
    ) {
        crate::evaluation::push_session_summary(actors, assets, page);
    }

    fn push_game_over(&self, actors: &mut Vec<Actor>, elapsed: f32) {
        let layout = Layout::current();
        actors.push(layout.solid([320.0, 240.0], [640.0, 480.0], [0.0, 0.0, 0.0, 1.0], -100));
        let mut sign = layout.sprite("stepmania-395/game-over", [320.0, 240.0], [512.0, 88.0], 0);
        sign.alpha(0.75 * (elapsed / 0.3).clamp(0.0, 1.0));
        actors.push(sign.build(0));
    }

    fn push_name_entry(&self, actors: &mut Vec<Actor>, page: NameEntryPage<'_>) {
        let layout = Layout::current();
        let editor = page.editor;
        if !editor.players.iter().any(|player| player.active) {
            return;
        }
        actors.push(
            layout
                .sprite(
                    "stepmania-395/name-entry",
                    [320.0, 240.0],
                    [640.0, 480.0],
                    -100,
                )
                .build(0),
        );
        if let Some(time) = TextContent::inline_format(format_args!(
            "{:02}",
            (editor.policy.timeout_seconds - editor.elapsed)
                .max(0.0)
                .ceil() as u32
        )) {
            actors.push(layout.text(TIMER_FONT, time, [320.0, 30.0], 1.0).build(0));
        }
        let beat = editor.elapsed * editor.policy.characters_per_second;
        let fraction = beat.fract();
        let offset = if fraction > 0.5 {
            1.0 - fraction
        } else {
            -fraction
        };
        let closest = editor.current_index();
        for (side, player) in editor
            .players
            .iter()
            .enumerate()
            .filter(|(_, player)| player.active)
        {
            let center = if page.wide {
                320.0
            } else if side == 1 {
                480.0
            } else {
                160.0
            };
            let columns: &[f32] = if page.diagonal && page.wide {
                &[-216.0, -168.0, -72.0, -24.0, 24.0, 72.0, 168.0, 216.0]
            } else if page.diagonal {
                &[-96.0, -48.0, 48.0, 96.0]
            } else if page.wide {
                &[-224.0, -160.0, -96.0, -32.0, 32.0, 96.0, 160.0, 224.0]
            } else {
                &[-96.0, -32.0, 32.0, 96.0]
            };
            for (column, &x) in columns.iter().enumerate() {
                let mut receptor = layout.sprite(
                    "stepmania-395/training-receptor",
                    [center + x, 100.0],
                    [64.0; 2],
                    1,
                );
                let frame = if (0.1..0.9).contains(&beat.fract()) {
                    0.5
                } else {
                    0.0
                };
                receptor.customtexturerect([frame, 0.0, frame + 0.5, 1.0]);
                receptor.rotationz(if page.diagonal {
                    [-45.0, -135.0, 135.0, 45.0][column % 4]
                } else {
                    [-90.0, 0.0, 180.0, 90.0][column % 4]
                });
                if let Some(Some(press)) = player.last_press.get(column) {
                    receptor.glow([
                        1.0,
                        1.0,
                        1.0,
                        (1.0 - (editor.elapsed - press) / 0.2).clamp(0.0, 1.0),
                    ]);
                }
                actors.push(receptor.build(0));
            }
            for (slot, &selected) in player.letters.iter().enumerate() {
                let x = center + columns[slot.min(columns.len() - 1)];
                if !player.done {
                    for row in 0..10 {
                        let index = (closest + editor.policy.alphabet.len() + row - 2)
                            % editor.policy.alphabet.len();
                        let ch = editor.policy.alphabet[index];
                        if ch == b' ' {
                            continue;
                        }
                        let zoom = if index == closest {
                            1.5 - offset.abs()
                        } else {
                            1.0
                        };
                        let alpha = if row == 0 {
                            ((offset + 0.5) * 2.0).clamp(0.0, 1.0)
                        } else if row == 9 {
                            (1.0 - offset * 2.0).clamp(0.0, 1.0)
                        } else {
                            1.0
                        };
                        let mut letter = layout.text(
                            ENTRY_FONT,
                            TextContent::Owned(char::from(ch).to_string()),
                            [x, 100.0 + (offset + row as f32 - 2.0) * 40.0],
                            zoom,
                        );
                        letter.diffuse([0.6, 0.8, 0.8, alpha]);
                        letter.z(3);
                        actors.push(letter.build(0));
                    }
                }
                if selected != b' ' {
                    let mut letter = layout.text(
                        ENTRY_FONT,
                        TextContent::Owned(char::from(selected).to_string()),
                        [x, 100.0],
                        1.5,
                    );
                    letter.diffuse([0.8, 1.0, 1.0, 1.0]);
                    letter.z(4);
                    actors.push(letter.build(0));
                }
            }
            let records = page.records[side]
                .iter()
                .map(|record| {
                    format!(
                        "{} #{} in {} {}",
                        if record.personal { "PR" } else { "MR" },
                        record.rank,
                        record.title,
                        record.difficulty
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let mut feats = layout.text(
                WHEEL_FONT,
                TextContent::Owned(records),
                [center, 430.0],
                0.7,
            );
            feats.z(5);
            actors.push(feats.build(0));
        }
    }
}
