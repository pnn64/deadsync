use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::SongLuaCompileContext;
use crate::SongLuaNoteHideWindow;
use crate::SongLuaOverlayCommandBlock;
use crate::SongLuaOverlayMessageCommand;
use crate::SongLuaOverlayModelLayer;
use crate::SongLuaOverlayState;

use crate::graph_display_body_size;

use crate::song_lua_human_player_count;

use crate::theme_path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongLuaCapturedMessageCommands {
    pub commands: Vec<SongLuaOverlayMessageCommand>,
    pub sounds: Vec<(String, PathBuf)>,
    pub skipped: Vec<String>,
}

#[inline]
pub(crate) fn texture_path_key(path: &Path) -> Arc<str> {
    Arc::from(path.to_string_lossy().as_ref())
}

// Load-time queue clocks use the same replay frames as callbacks. Storage lives
// for one compilation; it never exists on a gameplay frame.
struct SongLuaQueueClock {
    frame: usize,
    remaining: f64,
    // A zero-time state becomes visible on its actor update. Following tweens
    // inherit its destination but cannot render that pose on an earlier frame.
    zero_frame: Option<usize>,
}

pub(crate) struct SongLuaCompileFrames {
    times: Vec<f64>,
    // Optional native actor delta and its active span in the global frame.
    actor_frames: Option<Vec<[f32; 2]>>,
}

impl SongLuaCompileFrames {
    fn time(&self, frame: usize) -> f64 {
        self.times.get(frame).copied().unwrap_or_else(|| {
            self.times.last().copied().unwrap_or(0.0) + (frame + 1 - self.times.len()) as f64 / 60.0
        })
    }
    fn delta(&self, frame: usize) -> f64 {
        if let Some(delta) = self
            .actor_frames
            .as_ref()
            .and_then(|frames| frames.get(frame))
        {
            return f64::from(delta[0]);
        }
        if frame == 0 {
            0.0
        } else {
            f64::from(self.time(frame) as f32 - self.time(frame - 1) as f32)
        }
    }
    fn global_remainder(&self, frame: usize, remaining: f64) -> f64 {
        self.actor_frames
            .as_ref()
            .and_then(|frames| frames.get(frame))
            .map_or(remaining, |[delta, span]| {
                if *delta > 0.0 {
                    remaining * f64::from(*span) / f64::from(*delta)
                } else {
                    0.0
                }
            })
    }
}

fn advance_queue_clock(
    frames: &SongLuaCompileFrames,
    clock: &mut SongLuaQueueClock,
    duration: f64,
    mut samples: Option<&mut Vec<[f32; 2]>>,
) {
    // Actor::UpdateTweening subtracts native floats on each frame. A double
    // clock can release a zero-time command early and change later RNG calls.
    let mut left = duration as f32;
    loop {
        while clock.remaining <= 0.0 {
            clock.frame += 1;
            clock.remaining = frames.delta(clock.frame);
        }
        if duration as f32 == 0.0 {
            clock.zero_frame = Some(clock.frame);
        }
        // A callback after its child can enqueue a zero-time state on the final
        // captured frame. Its start still belongs to the next update, even when
        // that update lies beyond the replay. Omitting this sample makes the
        // pending destination look immediate and overwrites the last draw.
        if let Some(samples) = &mut samples
            && samples.is_empty()
        {
            let start = frames.time(clock.frame)
                - if left == 0.0 {
                    0.0
                } else {
                    frames.global_remainder(clock.frame, clock.remaining)
                };
            let mut start = start as f32;
            if left > 0.0 && clock.frame > 0 {
                let prior = frames.time(clock.frame - 1) as f32;
                // A tiny remainder can put the next tween strictly after the
                // prior update while both timestamps round to the same float.
                // Preserve queue order: it cannot draw on that earlier frame.
                if frames.time(clock.frame) - frames.global_remainder(clock.frame, clock.remaining)
                    > f64::from(prior)
                    && start <= prior
                {
                    start = prior.next_up();
                }
            }
            if let Some(frame) = clock.zero_frame {
                start = start.max(frames.time(frame) as f32);
            }
            samples.push([start, left]);
        }
        let elapsed = left.min(clock.remaining as f32);
        left -= elapsed;
        clock.remaining = f64::from(clock.remaining as f32 - elapsed);
        if let Some(samples) = &mut samples
            && clock.frame < frames.times.len()
            && left != duration as f32
        {
            samples.push([frames.time(clock.frame) as f32, left]);
        }
        if left == 0.0 {
            break;
        }
        if clock.frame >= frames.times.len() {
            let count = (left * 60.0).floor() as usize;
            if count > 1 {
                clock.frame += count - 1;
                left -= (count - 1) as f32 / 60.0;
            }
        }
    }
}

pub use crate::frame_capture::StartupState as SongLuaStartupState;

fn startup_clock_frames(
    times: &[f64],
    path: &[[f32; 2]],
    inputs: Option<&[[f32; 2]]>,
) -> Vec<[f32; 2]> {
    let mut path = path.to_vec();
    times
        .iter()
        .enumerate()
        .map(|(frame, &time)| {
            let mut delta = if frame == 0 {
                0.0
            } else {
                time as f32 - times[frame - 1] as f32
            };
            let mut span = delta;
            if let Some(input) = inputs.and_then(|inputs| inputs.get(frame)) {
                [delta, span] = *input;
            }
            for [hibernate, rate] in &mut path {
                if *hibernate > 0.0 {
                    *hibernate -= delta;
                    if *hibernate > 0.0 {
                        return [0.0, 0.0];
                    }
                    let remainder = -*hibernate;
                    span = if delta > 0.0 {
                        span * (remainder / delta)
                    } else {
                        0.0
                    };
                    delta = remainder;
                    *hibernate = 0.0;
                }
                delta *= *rate;
            }
            [delta, span]
        })
        .collect()
}

pub(crate) fn bake_startup_tweens<'a>(
    states: impl Iterator<Item = &'a mut SongLuaStartupState>,
    replay: impl Iterator<Item = (f64, f64)>,
    actor_frames: Option<&[[f32; 2]]>,
) {
    let mut seconds = 0.0;
    let times = replay
        .map(|(_, delta)| {
            seconds += delta;
            seconds
        })
        .collect::<Vec<_>>();
    if times.len() < 2 {
        return;
    }
    let mut frames = SongLuaCompileFrames {
        times,
        actor_frames: None,
    };
    for state in states {
        if state.steps.is_empty() {
            continue;
        }
        frames.actor_frames = (actor_frames.is_some()
            || state
                .clock_path
                .iter()
                .any(|[sleep, rate]| *sleep > 0.0 || *rate != 1.0))
        .then(|| startup_clock_frames(&frames.times, &state.clock_path, actor_frames));
        let mut clock = SongLuaQueueClock {
            frame: 0,
            remaining: 0.0,
            zero_frame: None,
        };
        for (index, duration) in state.steps.iter().enumerate() {
            let capture = state
                .block_steps
                .iter()
                .any(|step| *step == Some(index + 1));
            let mut samples = Vec::new();
            advance_queue_clock(
                &frames,
                &mut clock,
                f64::from(*duration),
                capture.then_some(&mut samples),
            );
            if samples.is_empty() {
                continue;
            }
            let samples: std::sync::Arc<[[f32; 2]]> = samples.into();
            for (block, step) in state.blocks.iter_mut().zip(&state.block_steps) {
                if *step == Some(index + 1) {
                    block.progress = Some(samples.clone());
                }
            }
        }
    }
}

struct NativeGameplayInBitmap {
    key: &'static str,
    filename: &'static str,
}

static NATIVE_GAMEPLAY_IN_BITMAPS: [NativeGameplayInBitmap; 57] = [
    NativeGameplayInBitmap {
        key: "Combo 100milestone/explosion",
        filename: "Combo 100milestone/explosion.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Arrows/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Arrows/Combo 1000milestone swoosh.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Arrows/Combo 100milestone minisplode",
        filename: "_VisualStyles/Arrows/Combo 100milestone minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Arrows/Combo 100milestone splode",
        filename: "_VisualStyles/Arrows/Combo 100milestone splode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Bears/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Bears/Combo 1000milestone swoosh.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Bears/Combo 100milestone minisplode",
        filename: "_VisualStyles/Bears/Combo 100milestone minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Bears/Combo 100milestone splode",
        filename: "_VisualStyles/Bears/Combo 100milestone splode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Cats/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Cats/Combo 1000milestone swoosh.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Cats/Combo 100milestone minisplode",
        filename: "_VisualStyles/Cats/Combo 100milestone minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Cats/Combo 100milestone splode",
        filename: "_VisualStyles/Cats/Combo 100milestone splode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Ducks/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Ducks/Combo 1000milestone swoosh.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Ducks/Combo 100milestone minisplode",
        filename: "_VisualStyles/Ducks/Combo 100milestone minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Ducks/Combo 100milestone splode",
        filename: "_VisualStyles/Ducks/Combo 100milestone splode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Gay/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Gay/Combo 1000milestone swoosh.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Gay/Combo 100milestone minisplode",
        filename: "_VisualStyles/Gay/Combo 100milestone minisplode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Gay/Combo 100milestone splode",
        filename: "_VisualStyles/Gay/Combo 100milestone splode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Hearts/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Hearts/Combo 1000milestone swoosh.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Hearts/Combo 100milestone minisplode",
        filename: "_VisualStyles/Hearts/Combo 100milestone minisplode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Hearts/Combo 100milestone splode",
        filename: "_VisualStyles/Hearts/Combo 100milestone splode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Spooky/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Spooky/Combo 1000milestone swoosh.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Spooky/Combo 100milestone minisplode",
        filename: "_VisualStyles/Spooky/Combo 100milestone minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Spooky/Combo 100milestone splode",
        filename: "_VisualStyles/Spooky/Combo 100milestone splode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/SRPG10/Combo 1000milestone swoosh",
        filename: "_VisualStyles/SRPG10/Combo 1000milestone swoosh.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/SRPG10/Combo 100milestone minisplode",
        filename: "_VisualStyles/SRPG10/Combo 100milestone minisplode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/SRPG10/Combo 100milestone splode",
        filename: "_VisualStyles/SRPG10/Combo 100milestone splode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Stars/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Stars/Combo 1000milestone swoosh.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Stars/Combo 100milestone minisplode",
        filename: "_VisualStyles/Stars/Combo 100milestone minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Stars/Combo 100milestone splode",
        filename: "_VisualStyles/Stars/Combo 100milestone splode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Technique/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Technique/Combo 1000milestone swoosh.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Technique/Combo 100milestone minisplode",
        filename: "_VisualStyles/Technique/Combo 100milestone minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Technique/Combo 100milestone splode",
        filename: "_VisualStyles/Technique/Combo 100milestone splode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Thonk/Combo 1000milestone swoosh",
        filename: "_VisualStyles/Thonk/Combo 1000milestone swoosh.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Thonk/Combo 100milestone minisplode",
        filename: "_VisualStyles/Thonk/Combo 100milestone minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Thonk/Combo 100milestone splode",
        filename: "_VisualStyles/Thonk/Combo 100milestone splode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Arrows/GameplayIn minisplode",
        filename: "_VisualStyles/Arrows/GameplayIn minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Arrows/GameplayIn splode",
        filename: "_VisualStyles/Arrows/GameplayIn splode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Bears/GameplayIn minisplode",
        filename: "_VisualStyles/Bears/GameplayIn minisplode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Bears/GameplayIn splode",
        filename: "_VisualStyles/Bears/GameplayIn splode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Cats/GameplayIn minisplode",
        filename: "_VisualStyles/Cats/GameplayIn minisplode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Cats/GameplayIn splode",
        filename: "_VisualStyles/Cats/GameplayIn splode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Ducks/GameplayIn minisplode",
        filename: "_VisualStyles/Ducks/GameplayIn minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Ducks/GameplayIn splode",
        filename: "_VisualStyles/Ducks/GameplayIn splode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Gay/GameplayIn minisplode",
        filename: "_VisualStyles/Gay/GameplayIn minisplode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Gay/GameplayIn splode",
        filename: "_VisualStyles/Gay/GameplayIn splode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Hearts/GameplayIn minisplode",
        filename: "_VisualStyles/Hearts/GameplayIn minisplode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Hearts/GameplayIn splode",
        filename: "_VisualStyles/Hearts/GameplayIn splode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Spooky/ExtraSpooky/Bats",
        filename: "_VisualStyles/Spooky/ExtraSpooky/Bats (doubleres) 2x1.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Spooky/GameplayIn minisplode",
        filename: "_VisualStyles/Spooky/GameplayIn minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Spooky/GameplayIn splode",
        filename: "_VisualStyles/Spooky/GameplayIn splode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/SRPG10/GameplayIn minisplode",
        filename: "_VisualStyles/SRPG10/GameplayIn minisplode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/SRPG10/GameplayIn splode",
        filename: "_VisualStyles/SRPG10/GameplayIn splode.png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Stars/GameplayIn minisplode",
        filename: "_VisualStyles/Stars/GameplayIn minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Stars/GameplayIn splode",
        filename: "_VisualStyles/Stars/GameplayIn splode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Technique/GameplayIn minisplode",
        filename: "_VisualStyles/Technique/GameplayIn minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Technique/GameplayIn splode",
        filename: "_VisualStyles/Technique/GameplayIn splode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Thonk/GameplayIn minisplode",
        filename: "_VisualStyles/Thonk/GameplayIn minisplode (doubleres).png",
    },
    NativeGameplayInBitmap {
        key: "_VisualStyles/Thonk/GameplayIn splode",
        filename: "_VisualStyles/Thonk/GameplayIn splode (doubleres).png",
    },
];

pub(crate) fn native_gameplay_in_bitmap_filename(name: &str) -> Option<&'static str> {
    NATIVE_GAMEPLAY_IN_BITMAPS
        .iter()
        .find(|bitmap| bitmap.key == name || bitmap.filename == name)
        .map(|bitmap| bitmap.filename)
}

pub fn push_note_hide_window(
    out: &mut Vec<SongLuaNoteHideWindow>,
    player: usize,
    column: usize,
    beats_per_t: f32,
    start_index: usize,
    end_index: usize,
) {
    if let Some(window) =
        crate::note_hide_window_from_indices(player, column, beats_per_t, start_index, end_index)
    {
        out.push(window);
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongLuaFunctionActionCapture {
    pub overlay_blocks: Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>,
    pub tracked_blocks: Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>,
    pub broadcasts: Vec<(String, bool)>,
    pub sound_paths: Vec<PathBuf>,
    pub side_effects: i64,
}

pub fn actor_indices_for_pointers(
    len: usize,
    mut pointer_at: impl FnMut(usize) -> usize,
    actor_ptrs: &HashSet<usize>,
) -> Vec<usize> {
    (0..len)
        .filter(|&index| actor_ptrs.contains(&pointer_at(index)))
        .collect()
}

pub fn actor_pointers_touch_actor(
    len: usize,
    mut pointer_at: impl FnMut(usize) -> usize,
    actor_ptrs: &[usize],
) -> bool {
    const LINEAR_MAX: usize = 8;
    if actor_ptrs.is_empty() {
        return false;
    }
    if actor_ptrs.len() <= LINEAR_MAX {
        return (0..len).any(|index| actor_ptrs.contains(&pointer_at(index)));
    }
    // Dense mod charts can probe hundreds of actor tables at once. Keep the
    // compilation scratch inline through that range; pathological charts spill
    // once here, never on a gameplay frame.
    let mut sorted = smallvec::SmallVec::<[usize; 512]>::from_slice(actor_ptrs);
    sorted.sort_unstable();
    sorted.dedup();
    (0..len).any(|index| sorted.binary_search(&pointer_at(index)).is_ok())
}

#[must_use]
pub fn read_graph_display_size(
    state: SongLuaOverlayState,
    context: &SongLuaCompileContext,
) -> [f32; 2] {
    let fallback = graph_display_body_size(song_lua_human_player_count(context));
    let size = state.size.unwrap_or(fallback);
    [size[0].abs().max(1.0), size[1].abs().max(1.0)]
}

pub fn overlay_model_layers_from_slots<Slot, Vertex, ReadLayer>(
    slots: &[Slot],
    read_layer: ReadLayer,
) -> Option<Arc<[SongLuaOverlayModelLayer<Vertex>]>>
where
    ReadLayer: Fn(&Slot) -> Option<SongLuaOverlayModelLayer<Vertex>>,
{
    let mut layers = Vec::with_capacity(slots.len());
    for slot in slots.iter() {
        let Some(layer) = read_layer(slot) else {
            continue;
        };
        layers.push(layer);
    }
    if layers.is_empty() {
        None
    } else {
        Some(Arc::from(layers.into_boxed_slice()))
    }
}

pub struct SongLuaNoteskinTapActorModel {
    pub skin: String,
    pub element: String,
    pub model_paths: [PathBuf; 3],
}

#[must_use]
pub fn banner_sort_order_path(sort_order: &str) -> Option<String> {
    let short = sort_order
        .trim()
        .split_once('_')
        .map(|(_, short)| short)
        .unwrap_or_else(|| sort_order.trim());
    let short = short
        .strip_suffix("_P1")
        .or_else(|| short.strip_suffix("_P2"))
        .unwrap_or(short);
    match short {
        "Group" => None,
        "" | "Invalid" => Some(theme_path("G", "Common", "fallback banner")),
        sort => Some(theme_path("G", "Banner", sort)),
    }
}
