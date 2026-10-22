// Frozen conversion from 276198421; only test visibility differs.
use super::*;

pub(super) fn convert(capture: Capture, sources: &FxHashMap<usize, DrawSource>) -> Result<Vec<(usize, DrawFrame)>, String> {
    let overlay_index = |pointer| match sources.get(&pointer) {
        Some(DrawSource::Overlay(index)) => Ok(*index),
        _ => Err("custom draw references an uncompiled capture or callback owner".to_owned()),
    };
    let mut owners = Vec::new();
    for frame in capture.frames.iter().filter(|frame| !frame.immediate) {
        for &owner in &frame.owners {
            if !owners.contains(&owner) {
                owners.push(owner);
            }
        }
    }
    let mut frames = Vec::new();
    for raw in capture.frames {
        let mut frame_owners = raw.owners;
        for &owner in owners.iter().filter(|_| !raw.immediate) {
            if !frame_owners.contains(&owner) {
                frame_owners.push(owner);
            }
        }
        let start = frames.len();
        // With one owner every valid call belongs to that frame. Quiet tails
        // reserve nothing; multiple owners retain their independent growth.
        let command_capacity = if frame_owners.len() == 1 { raw.calls.len() } else { 0 };
        for pointer in frame_owners {
            let owner = overlay_index(pointer)?;
            frames.push((owner, DrawFrame {
                second: raw.second,
                immediate: raw.immediate,
                owners: vec![owner],
                owner_ends: Vec::with_capacity(1),
                ops: Vec::with_capacity(command_capacity),
            }));
        }
        let by_owner = &mut frames[start..];
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
                            capture
                                .labels
                                .get(&pointer)
                                .map_or("<unnamed>", String::as_str),
                            raw.second,
                        )
                    })?,
                    state,
                    parents: context.parents,
                    camera: context.camera,
                    vertices,
                },
            };
            if let Some((_, frame)) = by_owner.iter_mut().find(|(index, _)| *index == owner) {
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
