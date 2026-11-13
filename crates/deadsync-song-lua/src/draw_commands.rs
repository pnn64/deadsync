//! Immutable custom draw payloads and source resolution, independent of Lua.
use crate::{SongLuaOverlayMeshVertex, SongLuaOverlayState};
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawSource {
    Overlay(usize),
    Player(usize),
    Judgment(usize),
    Combo(usize),
    SongForeground,
    ScreenLayer(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum DrawOp {
    Begin {
        capture: usize,
        preserve: bool,
    },
    Finish {
        capture: usize,
    },
    Draw {
        source: DrawSource,
        state: SongLuaOverlayState,
        /// Caller draw stack, outermost first; BeginRenderingTo resets it.
        parents: Arc<[SongLuaOverlayState]>,
        camera: Option<SongLuaOverlayState>,
        vertices: Option<Arc<[SongLuaOverlayMeshVertex]>>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawFrame {
    pub second: f32,
    /// A command/update submission, played once before ordinary scene drawing.
    /// It does not replace the owner's normal DrawPrimitives traversal.
    pub immediate: bool,
    pub owners: Vec<usize>,
    /// Exclusive operation end for each owner, in the same order as `owners`.
    pub owner_ends: Vec<usize>,
    pub ops: Vec<DrawOp>,
}

#[derive(Clone, Default)]
pub(crate) struct Context {
    pub(crate) owner: usize,
    pub(crate) parents: Arc<[SongLuaOverlayState]>,
    pub(crate) camera: Option<SongLuaOverlayState>,
}

pub(crate) enum RawOp {
    Begin(usize, bool),
    Finish(usize),
    Draw(
        usize,
        SongLuaOverlayState,
        Context,
        Option<Arc<[SongLuaOverlayMeshVertex]>>,
    ),
}

pub(crate) struct RawCall {
    pub(crate) owner: usize,
    pub(crate) op: RawOp,
}

pub(crate) struct RawFrame {
    pub(crate) second: f32,
    pub(crate) immediate: bool,
    pub(crate) owners: Vec<usize>,
    pub(crate) calls: Vec<RawCall>,
}

/// Song-load worker only, no synchronization or gameplay maintenance. Retains
/// immutable frames until compilation hands them to song-lifetime playback.
/// At most 256 operations/frame and 256 MiB/song, including vertex payloads
/// and caller stacks. Overflow reports unsupported capture; it never prunes or
/// silently reuses the final pass. No I/O or GPU work occurs here.
#[derive(Default)]
pub(crate) struct Capture {
    pub(crate) context: Option<Context>,
    pub(crate) targets: Vec<(usize, Context)>,
    pub(crate) calls: Vec<RawCall>,
    pub(crate) owners: Vec<usize>,
    pub(crate) frames: Vec<RawFrame>,
    pub(crate) labels: FxHashMap<usize, String>,
    pub(crate) bytes: usize,
    pub(crate) frame_ops: usize,
}

pub(crate) const MAX_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const MAX_OPS: usize = 256;
pub(crate) const DRAW_INSTALLED_KEY: &str = "__songlua_draw_installed";

// Actor::PreDraw multiplies diffuse by its inherited diffuse and combines
// glow with Screen blending before Actor::PartiallyOpaque tests the result.
pub(crate) fn partially_opaque(
    state: SongLuaOverlayState,
    parents: &[SongLuaOverlayState],
) -> bool {
    let mut diffuse = 1.0;
    let mut glow = 0.0;
    for parent in parents {
        diffuse *= parent
            .vertex_colors
            .map_or(parent.diffuse[3], |colors| colors[0][3]);
        glow = glow + parent.glow[3] - glow * parent.glow[3];
    }
    let diffuse_visible = state
        .vertex_colors
        .map_or(state.diffuse[3] * diffuse > 0.0, |colors| {
            colors.iter().any(|color| color[3] * diffuse > 0.0)
        });
    diffuse_visible || state.glow[3] + glow - state.glow[3] * glow > 0.0
}

pub(crate) fn convert(
    raw_frames: Vec<RawFrame>,
    labels: &FxHashMap<usize, String>,
    sources: &FxHashMap<usize, DrawSource>,
) -> Result<Vec<(usize, DrawFrame)>, String> {
    let overlay_index = |pointer| match sources.get(&pointer) {
        Some(DrawSource::Overlay(index)) => Ok(*index),
        _ => Err("custom draw references an uncompiled capture or callback owner".to_owned()),
    };
    let mut owners = Vec::new();
    for frame in raw_frames.iter().filter(|frame| !frame.immediate) {
        for &owner in &frame.owners {
            if !owners.contains(&owner) {
                owners.push(owner);
            }
        }
    }
    // Normal frames include the union of callback owners, even quiet tails.
    // Duplicate authored pointers may need extra slots and retain Vec growth.
    let capacity = raw_frames
        .iter()
        .map(|frame| {
            if frame.immediate {
                frame.owners.len()
            } else {
                frame.owners.len().max(owners.len())
            }
        })
        .sum();
    let mut frames = Vec::with_capacity(capacity);
    let mut positions = FxHashMap::<usize, usize>::default();
    let mut counts = Vec::new();
    for raw in raw_frames {
        let mut frame_owners = raw.owners;
        for &owner in owners.iter().filter(|_| !raw.immediate) {
            if !frame_owners.contains(&owner) {
                frame_owners.push(owner);
            }
        }
        let start = frames.len();
        // With one owner every valid call belongs to that frame. Quiet tails
        // reserve nothing; multiple owners retain their independent growth.
        let command_capacity = if frame_owners.len() == 1 {
            raw.calls.len()
        } else {
            0
        };
        let multiple = frame_owners.len() > 1 && !raw.calls.is_empty();
        if multiple {
            positions.clear();
            positions.reserve(frame_owners.len());
            counts.clear();
            counts.resize(frame_owners.len(), 0usize);
        }
        for pointer in frame_owners {
            let owner = overlay_index(pointer)?;
            // Aliased source bindings and duplicate owners route to the first
            // output frame, matching the original ordered search.
            if multiple {
                positions.entry(owner).or_insert(frames.len() - start);
            }
            frames.push((
                owner,
                DrawFrame {
                    second: raw.second,
                    immediate: raw.immediate,
                    owners: vec![owner],
                    owner_ends: Vec::with_capacity(1),
                    ops: Vec::with_capacity(command_capacity),
                },
            ));
        }
        let by_owner = &mut frames[start..];
        if multiple {
            // This sizing pass must not validate calls: source/target errors
            // still occur in operation order in the consuming loop below.
            for call in &raw.calls {
                if let Some(DrawSource::Overlay(owner)) = sources.get(&call.owner)
                    && let Some(&index) = positions.get(owner)
                {
                    counts[index] += 1;
                }
            }
            for ((_, frame), count) in by_owner.iter_mut().zip(&counts) {
                frame.ops.reserve_exact(*count);
            }
        }
        for call in raw.calls {
            let owner = overlay_index(call.owner)?;
            let op = match call.op {
                RawOp::Begin(pointer, preserve) => DrawOp::Begin {
                    capture: overlay_index(pointer)?,
                    preserve,
                },
                RawOp::Finish(pointer) => DrawOp::Finish {
                    capture: overlay_index(pointer)?,
                },
                RawOp::Draw(pointer, state, context, vertices) => DrawOp::Draw {
                    source: *sources.get(&pointer).ok_or_else(|| {
                        format!(
                            "custom draw references an uncompiled actor {} at {:.6}s",
                            labels.get(&pointer).map_or("<unnamed>", String::as_str),
                            raw.second,
                        )
                    })?,
                    state,
                    parents: context.parents,
                    camera: context.camera,
                    vertices,
                },
            };
            let slot = if multiple {
                positions.get(&owner).copied()
            } else {
                by_owner
                    .first()
                    .filter(|(index, _)| *index == owner)
                    .map(|_| 0)
            };
            if let Some(slot) = slot {
                let frame = &mut by_owner[slot].1;
                frame.ops.push(op);
            } else {
                return Err("custom draw lost its callback owner".to_owned());
            }
        }
        for (_, frame) in by_owner {
            frame.owner_ends.push(frame.ops.len());
        }
    }
    Ok(frames)
}
