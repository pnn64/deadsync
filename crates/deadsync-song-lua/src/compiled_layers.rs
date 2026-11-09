//! Shared message merging and compiled layer ownership, independent of a VM.
use crate::{
    CompiledSongLuaData, SongLuaMessageEvent, SongLuaOverlayActor, SongLuaOverlayKind,
    sort_compiled_song_lua,
};
use deadlib_present::actors::TextAttribute;
type DefaultCompiledSongLua<S, V, L> =
    CompiledSongLuaData<SongLuaOverlayActor<SongLuaOverlayKind<S, V, TextAttribute>>, L>;

pub(crate) fn merge_runtime_messages(
    messages: &mut Vec<SongLuaMessageEvent>,
    action_start: usize,
    runtime_broadcasts: &[(f32, String, bool)],
) {
    if runtime_broadcasts.is_empty() {
        return;
    }
    let action_end = messages.len();
    if action_end - action_start < 32 || runtime_broadcasts.len() < 16 {
        return merge_runtime_messages_linear(messages, action_start, runtime_broadcasts);
    }
    let mut by_name = rustc_hash::FxHashMap::<&str, Vec<&mut f32>>::default();
    for event in &mut messages[action_start..action_end] {
        if !event.message.starts_with("__songlua_overlay_fn_action_") && !event.beat.is_nan() {
            by_name
                .entry(event.message.as_str())
                .or_default()
                .push(&mut event.beat);
        }
    }
    for beats in by_name.values_mut() {
        // Stable sorting leaves the last original index last when beats tie.
        beats.sort_by(|a, b| (**a).total_cmp(&**b));
    }
    let mut missing = Vec::new();
    for (beat, name, _) in runtime_broadcasts {
        if let Some(beats) = by_name.get_mut(name.as_str()) {
            let end = beats.partition_point(|scheduled| **scheduled <= *beat + f32::EPSILON);
            if end != 0 {
                *beats.remove(end - 1) = *beat;
                continue;
            }
        }
        missing.push(SongLuaMessageEvent {
            beat: *beat,
            message: name.clone(),
            persists: false,
        });
    }
    // Appended events never participate in the original action window.
    drop(by_name);
    messages.extend(missing);
}

fn merge_runtime_messages_linear(
    messages: &mut Vec<SongLuaMessageEvent>,
    action_start: usize,
    runtime_broadcasts: &[(f32, String, bool)],
) {
    let action_end = messages.len();
    let mut matched = vec![false; action_end - action_start];
    for (runtime_beat, runtime_message, _) in runtime_broadcasts {
        let Some(index) = messages
            .iter()
            .enumerate()
            .take(action_end)
            .skip(action_start)
            .filter(|(index, event)| {
                !matched[*index - action_start]
                    && !event.message.starts_with("__songlua_overlay_fn_action_")
                    && event.message == *runtime_message
                    && event.beat <= *runtime_beat + f32::EPSILON
            })
            .max_by(|(_, left), (_, right)| left.beat.total_cmp(&right.beat))
            .map(|(index, _)| index)
        else {
            // MessageManager::Broadcast dispatches every runtime call, including
            // schedules stored under local or chart-specific table names. The
            // sampled tracks retain parameter-dependent listener writes.
            messages.push(SongLuaMessageEvent {
                beat: *runtime_beat,
                message: runtime_message.clone(),
                persists: false,
            });
            continue;
        };
        messages[index].beat = *runtime_beat;
        matched[index - action_start] = true;
    }
}

// Preserve an actor's original sibling position when later wrappers surround it.
// This load-time traversal emits parents first, as composition and camera scopes
// require, and rejects cycles instead of leaving forward parents unresolved.
pub(crate) fn wrapper_overlay_order<NoteskinSlot, ModelVertex>(
    overlays: &[SongLuaOverlayActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
) -> Result<Vec<usize>, String> {
    if overlays
        .iter()
        .enumerate()
        .all(|(index, overlay)| overlay.parent_index.is_none_or(|parent| parent < index))
    {
        return Ok((0..overlays.len()).collect());
    }
    let count = overlays.len();
    // Parent zero is the root list; parent i + 1 owns actor i.
    // Keep all child lists in one buffer instead of allocating a Vec per parent.
    let mut offsets = vec![0usize; count + 2];
    for overlay in overlays {
        let parent = overlay.parent_index.map_or(0, |parent| parent + 1);
        let Some(size) = offsets.get_mut(parent + 1) else {
            return Err("Runtime wrapper has an invalid parent".into());
        };
        *size += 1;
    }
    for index in 1..offsets.len() {
        offsets[index] += offsets[index - 1];
    }
    let mut children = vec![0usize; count];
    // Fill from the end so each list retains original sibling order.
    // Consumed ends become starts; shift them back to the parent-key positions.
    for (index, overlay) in overlays.iter().enumerate().rev() {
        let parent = overlay.parent_index.map_or(0, |parent| parent + 1);
        offsets[parent + 1] -= 1;
        children[offsets[parent + 1]] = index;
    }
    for parent in 0..=count {
        offsets[parent] = offsets[parent + 1];
    }
    offsets[count + 1] = count;
    let mut source = overlays
        .iter()
        .enumerate()
        .map(|(index, overlay)| {
            if matches!(overlay.kind, SongLuaOverlayKind::WrapperState) {
                usize::MAX
            } else {
                index
            }
        })
        .collect::<Vec<_>>();
    let mut chain = Vec::new();
    for index in 0..count {
        chain.clear();
        let mut next = index;
        while source[next] == usize::MAX {
            if chain.len() >= count {
                return Err("Cycle in runtime wrapper ownership".into());
            }
            chain.push(next);
            let Some(&child) = children[offsets[next + 1]..offsets[next + 2]].first() else {
                source[next] = next;
                break;
            };
            next = child;
        }
        for index in chain.drain(..) {
            source[index] = source[next];
        }
    }
    for parent in 0..=count {
        children[offsets[parent]..offsets[parent + 1]].sort_by_key(|&index| source[index]);
    }
    let mut order = Vec::with_capacity(count);
    let mut pending = children[offsets[0]..offsets[1]]
        .iter()
        .rev()
        .copied()
        .collect::<Vec<_>>();
    while let Some(index) = pending.pop() {
        if order.len() >= count {
            return Err("Cycle in runtime wrapper hierarchy".into());
        }
        order.push(index);
        pending.extend(
            children[offsets[index + 1]..offsets[index + 2]]
                .iter()
                .rev()
                .copied(),
        );
    }
    if order.len() != count {
        return Err("Runtime wrapper hierarchy has no reachable root".into());
    }
    Ok(order)
}

pub(crate) fn split_compiled_song_lua<NoteskinSlot, ModelVertex, LiveScene>(
    mut compiled: DefaultCompiledSongLua<NoteskinSlot, ModelVertex, LiveScene>,
    overlay_layers: Vec<usize>,
    entry_paths: &[std::path::PathBuf],
    primary_index: usize,
) -> Result<Vec<DefaultCompiledSongLua<NoteskinSlot, ModelVertex, LiveScene>>, String> {
    if overlay_layers.len() != compiled.overlays.len() {
        return Err("song lua overlay ownership did not match compiled overlays".to_string());
    }
    let overlay_order = wrapper_overlay_order(&compiled.overlays)?;
    let mut local_counts = vec![0usize; entry_paths.len()];
    let mut overlay_map = vec![(0usize, 0usize); compiled.overlays.len()];
    for &index in &overlay_order {
        let layer = overlay_layers[index];
        if layer >= local_counts.len() {
            return Err(format!("song lua overlay has invalid layer index {layer}"));
        }
        overlay_map[index] = (layer, local_counts[layer]);
        local_counts[layer] += 1;
    }
    let mut outputs = entry_paths
        .iter()
        .enumerate()
        .map(|(layer, entry_path)| DefaultCompiledSongLua {
            overlays: Vec::with_capacity(local_counts[layer]),
            live_indices: if compiled.live_scene.is_some() {
                Vec::with_capacity(local_counts[layer])
            } else {
                Vec::new()
            },
            entry_path: entry_path.clone(),
            loaded_steps: compiled.loaded_steps,
            loaded_style: if layer + 1 == entry_paths.len() {
                compiled.loaded_style.take()
            } else {
                compiled.loaded_style.clone()
            },
            startup_reloads: compiled.startup_reloads,
            screen_width: compiled.screen_width,
            screen_height: compiled.screen_height,
            overlay_update_unit: compiled.overlay_update_unit,
            messages: if layer + 1 == entry_paths.len() {
                std::mem::take(&mut compiled.messages)
            } else {
                compiled.messages.clone()
            },
            sound_paths: if layer + 1 == entry_paths.len() {
                std::mem::take(&mut compiled.sound_paths)
            } else {
                compiled.sound_paths.clone()
            },
            ..DefaultCompiledSongLua::default()
        })
        .collect::<Vec<_>>();

    if let Some(index) = compiled.screen_overlay_index {
        let (layer, local) = overlay_map[index];
        outputs[layer].screen_overlay_index = Some(local);
    }

    for mut frame in compiled.draw_frames.drain(..) {
        let owner = *frame
            .owners
            .first()
            .ok_or("custom draw frame has no owner")?;
        let (layer, _) = overlay_map[owner];
        let remap = |index: &mut usize| -> Result<(), String> {
            let &(target_layer, local) = overlay_map
                .get(*index)
                .ok_or("custom draw has an invalid overlay index")?;
            if target_layer != layer {
                return Err("custom draw crosses separately played song layers".to_owned());
            }
            *index = local;
            Ok(())
        };
        for owner in &mut frame.owners {
            remap(owner)?;
        }
        for op in &mut frame.ops {
            match op {
                crate::SongLuaDrawOp::Begin { capture, .. }
                | crate::SongLuaDrawOp::Finish { capture } => remap(capture)?,
                crate::SongLuaDrawOp::Draw {
                    source: crate::SongLuaDrawSource::Overlay(index),
                    ..
                } => remap(index)?,
                _ => {}
            }
        }
        let frames = &mut outputs[layer].draw_frames;
        // Repeated immediate Draw calls may have the same owner and clock;
        // keep each submission independent rather than merging owner batches.
        if let Some(last) = frames
            .last_mut()
            .filter(|last| !frame.immediate && !last.immediate && last.second == frame.second)
        {
            last.owner_ends
                .extend(frame.owner_ends.iter().map(|end| last.ops.len() + end));
            last.owners.extend(frame.owners);
            last.ops.extend(frame.ops);
        } else {
            frames.push(frame);
        }
    }

    let mut overlays = compiled.overlays.drain(..).map(Some).collect::<Vec<_>>();
    for global_index in overlay_order {
        // The validated traversal visits each owned node exactly once.
        let mut overlay = overlays[global_index]
            .take()
            .expect("unique overlay traversal");
        let (layer, _) = overlay_map[global_index];
        overlay.parent_index = overlay.parent_index.and_then(|parent| {
            overlay_map
                .get(parent)
                .filter(|(parent_layer, _)| *parent_layer == layer)
                .map(|(_, local)| *local)
        });
        if let SongLuaOverlayKind::ActorProxy {
            target: crate::SongLuaProxyTarget::Actor { overlay_index },
        } = &mut overlay.kind
        {
            *overlay_index = overlay_map
                .get(*overlay_index)
                .filter(|(target_layer, _)| *target_layer == layer)
                .map_or(usize::MAX, |(_, local)| *local);
        }
        outputs[layer].overlays.push(overlay);
        if compiled.live_scene.is_some() {
            outputs[layer].live_indices.push(global_index);
        }
    }
    for mut ease in compiled.overlay_eases.drain(..) {
        let Some(&(layer, local)) = overlay_map.get(ease.overlay_index) else {
            continue;
        };
        ease.overlay_index = local;
        outputs[layer].overlay_eases.push(ease);
    }
    for mut update in compiled.overlay_updates.drain(..) {
        let Some(&(layer, local)) = overlay_map.get(update.overlay_index) else {
            continue;
        };
        update.overlay_index = local;
        outputs[layer].overlay_updates.push(update);
    }
    #[cfg(feature = "test-support")]
    for mut write in compiled.overlay_writes.drain(..) {
        let Some(&(layer, local)) = overlay_map.get(write.overlay_index) else {
            continue;
        };
        write.overlay_index = local;
        outputs[layer].overlay_writes.push(write);
    }
    for capture in compiled.stateful_message_captures.drain(..) {
        let mut targets_by_layer = vec![Vec::new(); outputs.len()];
        for (global, targets) in capture.overlay_targets {
            let Some(&(layer, local)) = overlay_map.get(global) else {
                continue;
            };
            targets_by_layer[layer].push((local, targets));
        }
        let mut writes_by_layer = vec![Vec::new(); outputs.len()];
        for mut write in capture.writes {
            let Some(&(layer, local)) = overlay_map.get(write.overlay_index) else {
                continue;
            };
            write.overlay_index = local;
            writes_by_layer[layer].push(write);
        }
        for (layer, overlay_targets) in targets_by_layer.into_iter().enumerate() {
            if overlay_targets.is_empty() {
                continue;
            }
            outputs[layer]
                .stateful_message_captures
                .push(crate::SongLuaStatefulMessageCapture {
                    message: capture.message.clone(),
                    overlay_targets,
                    writes: std::mem::take(&mut writes_by_layer[layer]),
                });
        }
    }

    let primary = &mut outputs[primary_index];
    primary.live_scene = compiled.live_scene.take();
    #[cfg(feature = "test-support")]
    {
        primary.boolean_writes = compiled.boolean_writes;
        primary.speed_writes = compiled.speed_writes;
        primary.state_calls = compiled.state_calls;
        primary.noteskin_writes = compiled.noteskin_writes;
    }
    primary.startup = compiled.startup;
    primary.beat_mods = compiled.beat_mods;
    primary.time_mods = compiled.time_mods;
    primary.eases = compiled.eases;
    primary.player_actors = compiled.player_actors;
    primary.song_foreground = compiled.song_foreground;
    primary.screen_layers = compiled.screen_layers;
    primary.hidden_players = compiled.hidden_players;
    primary.note_hides = compiled.note_hides;
    primary.column_offsets = compiled.column_offsets;
    primary.column_splines = compiled.column_splines;
    primary.column_spline_origin = compiled.column_spline_origin;
    primary.info = compiled.info;
    for output in &mut outputs {
        sort_compiled_song_lua(output);
    }
    Ok(outputs)
}
