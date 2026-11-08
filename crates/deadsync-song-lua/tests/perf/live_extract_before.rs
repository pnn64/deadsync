// Frozen from ade130230; test-only output and measurement reference.
use super::*;

pub(super) fn take_live(lua: &Lua) -> Result<Vec<DrawFrame>, String> {
    if lua.app_data_ref::<Capture>().is_none_or(|capture| {
        !capture.frames.iter().any(|frame| frame.immediate) || capture.context.is_some()
    }) {
        return Ok(Vec::new());
    }
    if lua.app_data_ref::<LiveSources>().is_none() {
        return Ok(Vec::new());
    }
    let mut capture = lua
        .remove_app_data::<Capture>()
        .expect("complete capture frames");
    let (immediate, normal) = std::mem::take(&mut capture.frames)
        .into_iter()
        .partition::<Vec<_>, _>(|frame| frame.immediate);
    let frozen = Capture {
        frames: immediate,
        labels: capture.labels.clone(),
        ..Capture::default()
    };
    capture.frames = normal;
    if capture.frames.is_empty() && capture.calls.is_empty() && capture.targets.is_empty() {
        capture.bytes = 0;
        capture.frame_ops = 0;
        capture.labels.clear();
    }
    lua.set_app_data(capture);
    let sources = lua
        .app_data_ref::<LiveSources>()
        .expect("live source bindings");
    convert(frozen, &sources.0).map(|frames| frames.into_iter().map(|(_, frame)| frame).collect())
}

pub(super) fn convert(
    capture: Capture,
    sources: &FxHashMap<usize, DrawSource>,
) -> Result<Vec<(usize, DrawFrame)>, String> {
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
    // Normal frames include the union of callback owners, even quiet tails.
    // Duplicate authored pointers may need extra slots and retain Vec growth.
    let capacity = capture
        .frames
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

pub(super) fn actor_tostring(_: &Lua, actor: Table) -> mlua::Result<String> {
    let kind = actor
        .raw_get::<Option<String>>("__songlua_actor_type")?
        .unwrap_or_else(|| "Actor".into());
    let kind = match kind.as_str() {
        "Quad" => "Sprite",
        "WrapperState" => "ActorFrame",
        "TopScreen" => "ScreenGameplay",
        kind => kind,
    };
    // Luna::tostring_T exposes only type and opaque object identity.
    #[cfg(target_os = "windows")]
    let identity = format!("{:016X}", actor.to_pointer() as usize);
    #[cfg(not(target_os = "windows"))]
    let identity = format!("{:p}", actor.to_pointer());
    Ok(format!("{kind} ({identity})"))
}
